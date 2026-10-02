//! Alternative propagation modes for the `Fuzzy` type: Monte Carlo
//! sampling and the unscented transform.
//!
//! The `Fuzzy` operators use first-order (delta-method) propagation, which
//! is exact for linear maps and degrades for nonlinear ones. These modes
//! trade work for accuracy:
//!
//! * [`monte_carlo`] draws `(x, y)` pairs from the two Gaussian models,
//!   pushes them through `f`, and reports the sample moments (Welford's
//!   online algorithm, so no catastrophic cancellation in
//!   `E[x²] − E[x]²`). Converges at the usual `1/√N` rate; accuracy is a
//!   statistical property, checked with fixed seeds.
//! * [`unscented`] evaluates `f` at `2n+1` deterministic sigma points
//!   (`n = 2` inputs, standard weights with λ = `3 − n`) and recovers the
//!   moments in closed form. Five evaluations, and — unlike the delta
//!   method — the mean estimate *sees the curvature* of `f` at second
//!   order.
//!
//! Both assume independent inputs (the same assumption as the operators)
//! and take the RNG as an `FnMut() -> f64` uniform generator, the same
//! pattern `Distribution::sample` uses — the core keeps no RNG dependency.

use crate::fuzzy::Fuzzy;
use crate::quants::norm_ppf;
use num_traits::{Float, FromPrimitive};

/// One standard-normal draw from a uniform via the quantile function.
fn standard_draw(uniform: f64) -> f64 {
    let u = uniform.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
    norm_ppf(u)
}

/// Monte Carlo propagation of `f` through two independent Gaussian
/// estimates.
///
/// Draws `samples` `(x, y)` pairs, applies `f`, and returns the sample
/// mean and variance (Welford). `rng` is a uniform `(0, 1)` generator; two
/// draws are consumed per sample. With a fixed seed the result is
/// reproducible; across seeds it scatters at the usual `1/√samples` rate —
/// treat the variance as having ~`2/samples` relative noise.
#[must_use]
pub fn monte_carlo<T>(
    a: &Fuzzy<T>,
    b: &Fuzzy<T>,
    samples: usize,
    f: impl Fn(T, T) -> T,
    mut rng: impl FnMut() -> f64,
) -> Fuzzy<T>
where
    T: Float + FromPrimitive,
{
    let (ma, sa) = (
        a.mean().to_f64().unwrap_or(0.0),
        a.standard_deviation().to_f64().unwrap_or(0.0),
    );
    let (mb, sb) = (
        b.mean().to_f64().unwrap_or(0.0),
        b.standard_deviation().to_f64().unwrap_or(0.0),
    );
    let mut count = 0_usize;
    let mut mean = T::zero();
    let mut m2 = T::zero();
    for _ in 0..samples {
        let x = T::from_f64(ma + sa * standard_draw(rng())).unwrap_or_else(T::zero);
        let y = T::from_f64(mb + sb * standard_draw(rng())).unwrap_or_else(T::zero);
        let out = f(x, y);
        count += 1;
        let delta = out - mean;
        // Loop counters stay far below 2^53 in any realistic run.
        #[allow(clippy::cast_precision_loss)]
        let n = count as f64;
        mean = mean + delta / T::from_f64(n).unwrap_or_else(T::zero);
        m2 = m2 + delta * (out - mean);
    }
    if count < 2 {
        return Fuzzy::new(mean, T::zero());
    }
    let n = T::from(count).unwrap_or_else(T::zero);
    // Unbiased sample variance.
    Fuzzy::new(mean, m2 / (n - T::one()))
}

/// Unscented-transform propagation of `f` through two independent Gaussian
/// estimates.
///
/// `2n + 1 = 5` sigma points at the mean and `±√(n+λ)σ` along each axis,
/// standard weights `w₀ = λ/(n+λ) = 1/3`, `wᵢ = 1/(2(n+λ)) = 1/6`
/// (λ = 1 for `n = 2`), then the exact weighted moments of the five
/// outputs.
///
/// Deterministic (no RNG), five evaluations of `f`, and second-order
/// accurate in the mean — for `f = exp`, the UT mean lands far closer to
/// the exact lognormal mean `e^{m+v/2}` than the delta method's `e^m`.
#[must_use]
pub fn unscented<T>(a: &Fuzzy<T>, b: &Fuzzy<T>, f: impl Fn(T, T) -> T) -> Fuzzy<T>
where
    T: Float + FromPrimitive,
{
    let two = T::one() + T::one();
    let n = two; // two inputs
    let lambda = T::from(3).unwrap_or_else(T::zero) - n; // the common default κ = 3 − n
    let scale = (n + lambda).sqrt();
    let w0 = lambda / (n + lambda);
    let wi = T::one() / (two * (n + lambda));

    let (ma, sa) = (a.mean(), a.standard_deviation());
    let (mb, sb) = (b.mean(), b.standard_deviation());

    // Sigma points: (m, m), (m ± √3·s_a, m), (m, m ± √3·s_b).
    let points = [
        (ma, mb),
        (ma + scale * sa, mb),
        (ma - scale * sa, mb),
        (ma, mb + scale * sb),
        (ma, mb - scale * sb),
    ];
    let weights = [w0, wi, wi, wi, wi];

    // Weighted mean…
    let mut mean = T::zero();
    for (&(x, y), &w) in points.iter().zip(&weights) {
        mean = mean + w * f(x, y);
    }
    // …and weighted variance.
    let mut variance = T::zero();
    for (&(x, y), &w) in points.iter().zip(&weights) {
        let d = f(x, y) - mean;
        variance = variance + w * d * d;
    }
    Fuzzy::new(mean, variance)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::Fuzzy;

    /// Deterministic uniform generator (xorshift), seeded per test.
    #[allow(clippy::cast_precision_loss)] // xorshift output, 53 usable bits
    fn rng(seed: u64) -> impl FnMut() -> f64 {
        let mut state = seed.max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1_u64 << 53) as f64
        }
    }

    #[test]
    fn monte_carlo_matches_closed_forms_for_linear_maps() {
        // f(x, y) = x + y: the delta method is exact, so MC must land on it
        // within statistical noise.
        let a = Fuzzy::new(10.0_f64, 1.0);
        let b = Fuzzy::new(2.0_f64, 0.25);
        let mc = monte_carlo(&a, &b, 200_000, |x, y| x + y, rng(0x5EED));
        assert!((mc.mean() - 12.0).abs() < 0.02, "{}", mc.mean());
        assert!(
            (mc.variance() - 1.25).abs() < 0.05,
            "variance {} vs 1.25",
            mc.variance()
        );
        // And for scaling: f = 3x.
        let mc = monte_carlo(&a, &b, 200_000, |x, _y| 3.0 * x, rng(0xBEEF));
        assert!((mc.mean() - 30.0).abs() < 0.05);
        assert!((mc.variance() - 9.0).abs() < 0.4, "{}", mc.variance());
    }

    #[test]
    fn unscented_is_exact_for_linear_maps() {
        let a = Fuzzy::new(10.0_f64, 1.0);
        let b = Fuzzy::new(2.0_f64, 0.25);
        let sum = unscented(&a, &b, |x, y| x + y);
        assert!((sum.mean() - 12.0).abs() < 1e-12);
        assert!((sum.variance() - 1.25).abs() < 1e-12);
        let prod = unscented(&a, &b, |x, y| x * y);
        // For products the delta method IS the second-order result, so the
        // UT must reproduce it (up to the O(v²) term it also sees).
        let delta = a * b;
        assert!((prod.mean() - delta.mean()).abs() < 1e-9);
    }

    #[test]
    fn unscented_sees_curvature_where_delta_does_not() {
        // exp(x) at x ~ N(0, v = 0.25): exact mean is the lognormal
        // e^{v/2} = e^{0.125} ≈ 1.1331. The delta method reports e^0 = 1.
        let x = Fuzzy::new(0.0_f64, 0.25);
        let zero = Fuzzy::new(0.0_f64, 0.0);
        let truth = (0.25_f64 / 2.0).exp();
        let ut = unscented(&x, &zero, |v, _w| v.exp());
        let delta = (x * zero).exp();
        assert!(delta.mean() == 1.0, "delta method misses curvature");
        assert!(
            (ut.mean() - truth).abs() < (1.0 - truth).abs(),
            "UT mean {} vs truth {truth}",
            ut.mean()
        );
    }

    #[test]
    fn monte_carlo_approaches_the_lognormal_truth() {
        let x = Fuzzy::new(0.0_f64, 0.25);
        let zero = Fuzzy::new(0.0_f64, 0.0);
        let truth = (0.25_f64 / 2.0).exp();
        let mc = monte_carlo(&x, &zero, 500_000, |v, _w| v.exp(), rng(42));
        assert!(
            (mc.mean() - truth).abs() < 0.01,
            "MC mean {} vs lognormal truth {truth}",
            mc.mean()
        );
    }

    #[test]
    fn mc_variance_is_non_negative_and_finite() {
        let a = Fuzzy::new(1.0_f64, 2.0);
        let mc = monte_carlo(&a, &a, 1_000, |x, y| x - y, rng(7));
        assert!(mc.variance() >= 0.0 && mc.variance().is_finite());
        // Degenerate sample count: no NaNs.
        let mc = monte_carlo(&a, &a, 0, |x, y| x + y, rng(7));
        assert!(mc.mean().is_finite() && mc.variance() >= 0.0);
    }
}
