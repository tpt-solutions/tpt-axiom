//! Monte Carlo simulation: draw many samples of an uncertain quantity,
//! propagate them through a computation with ordinary arithmetic, and check
//! that the *empirical* distribution of the results agrees with the
//! *analytically propagated* `Fuzzy<f64>` uncertainty.
//!
//! ```text
//! cargo run -p tpt-axiom --example monte_carlo
//! ```

// Deterministic RNG plumbing converts counters to `f64` and defines items
// after the doc-level statements; both are fine in example land.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::items_after_statements)]
// The simulation accumulates with plain `+`/`*` on purpose: it mirrors the
// arithmetic `Fuzzy<T>` propagates, rather than optimized `mul_add` forms.
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::uninlined_format_args)]

use tpt_axiom::prelude::*;

/// Deterministic splitmix64 `(0, 1)` uniform generator (hermetic examples).
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z ^= z >> 27;
        z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Standard-normal draw via Box–Muller.
    fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next().max(f64::EPSILON);
        let u2 = self.next().clamp(f64::EPSILON, 1.0);
        (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos()
    }
}

fn main() {
    // The model: a delivery drone flying at uncertain speed for an uncertain
    // time against an uncertain headwind offset.
    //
    //   distance = speed * time - wind
    //
    // Analytic propagation (what `Fuzzy<f64>` computes for us):
    //   var(speed * time) = var_s * var_t + var_s * m_t^2 + var_t * m_s^2
    //   var(result)       = var(speed * time) + var_wind
    let speed = Fuzzy::new(12.0_f64, 0.25); // m/s
    let time = Fuzzy::new(30.0_f64, 0.04); // s
    let wind = Fuzzy::new(15.0_f64, 0.09); // m
    let analytic = speed * time - wind;

    // Monte Carlo: 500k draws through the same arithmetic.
    const N: u64 = 500_000;
    let mut rng = SplitMix(0xDEAD_BEEF_CAFE_F00D);
    let mut sum = 0.0;
    let mut sumsq = 0.0;
    for _ in 0..N {
        let s = speed.mean() + rng.next_gaussian() * speed.standard_deviation();
        let t = time.mean() + rng.next_gaussian() * time.standard_deviation();
        let w = wind.mean() + rng.next_gaussian() * wind.standard_deviation();
        let x = s * t - w;
        sum += x;
        sumsq += x * x;
    }
    let empirical_mean = sum / N as f64;
    let empirical_var = sumsq / N as f64 - empirical_mean * empirical_mean;

    println!("distance = speed * time - wind  (N = {N} samples)");
    println!(
        "analytic:   mean = {:.4} m, variance = {:.4} m^2",
        analytic.mean(),
        analytic.variance()
    );
    println!(
        "monte carlo: mean = {:.4} m, variance = {:.4} m^2",
        empirical_mean, empirical_var
    );

    let mean_err = (analytic.mean() - empirical_mean).abs();
    let var_err = (analytic.variance() - empirical_var).abs() / analytic.variance();
    println!("agreement:  |Δmean| = {mean_err:.5} m, |Δvar| = {var_err:.3}% relative");

    assert!(mean_err < 0.05, "means should agree to well under the SE");
    assert!(var_err < 0.02, "variances should agree to within ~2%");
    println!("analytic error propagation matches simulation.");
}
