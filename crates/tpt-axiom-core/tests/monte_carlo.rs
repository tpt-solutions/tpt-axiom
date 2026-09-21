//! Monte Carlo cross-check: sample many independent draws, combine them with
//! the same operations as [`Fuzzy<T>`], and verify that the *empirical* mean
//! and variance agree with the *analytically propagated* values within a
//! tolerance that accounts for sampling noise.

use tpt_axiom_core::Fuzzy;

/// Deterministic splitmix64 `(0,1)` uniform generator (keeps tests hermetic).
struct SplitMix(u64);
impl SplitMix {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let x = self.0;
        let mut z = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z ^= z >> 27;
        z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Standard-normal draw via Box–Muller, using `next` for uniforms.
    fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next().max(f64::EPSILON);
        let u2 = self.next().max(f64::EPSILON).min(1.0);
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

fn sample_moments(samples: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut n = 0_u64;
    let mut sum = 0.0;
    let mut sumsq = 0.0;
    for x in samples {
        n += 1;
        sum += x;
        sumsq += x * x;
    }
    let mean = sum / n as f64;
    let var = sumsq / n as f64 - mean * mean;
    (mean, var)
}

fn assert_close(label: &str, got: f64, expected: f64, tol: f64) {
    assert!(
        (got - expected).abs() <= tol * expected.abs().max(1e-12),
        "{label}: got {got}, expected {expected} (rel tol {tol})"
    );
}

#[test]
fn monte_carlo_addition() {
    let n = 500_000_u64;
    let a = Fuzzy::new(10.5_f64, 0.5);
    let b = Fuzzy::new(2.0_f64, 0.1);
    let (em, ev) = {
        let mut rng = SplitMix(0xDEAD_BEEF);
        sample_moments((0..n).map(|_| {
            a.mean() + a.standard_deviation() * rng.next_gaussian()
                + b.mean() + b.standard_deviation() * rng.next_gaussian()
        }))
    };
    let c = a + b;
    assert_close("add mean", em, c.mean(), 1e-3);
    assert_close("add variance", ev, c.variance(), 1e-2);
}

#[test]
fn monte_carlo_multiplication() {
    let n = 500_000_u64;
    let a = Fuzzy::new(3.0_f64, 0.25);
    let b = Fuzzy::new(2.0_f64, 0.5);
    let (em, ev) = {
        let mut rng = SplitMix(0xBADC_0FFE);
        sample_moments((0..n).map(|_| {
            (a.mean() + a.standard_deviation() * rng.next_gaussian())
                * (b.mean() + b.standard_deviation() * rng.next_gaussian())
        }))
    };
    let c = a * b;
    assert_close("mul mean", em, c.mean(), 2e-3);
    assert_close("mul variance", ev, c.variance(), 2e-2);
}

#[test]
fn monte_carlo_division() {
    let n = 500_000_u64;
    let a = Fuzzy::new(8.0_f64, 1.0);
    let b = Fuzzy::new(2.0_f64, 0.25);
    let (em, ev) = {
        let mut rng = SplitMix(0xF00D_DEAD);
        sample_moments((0..n).map(|_| {
            (a.mean() + a.standard_deviation() * rng.next_gaussian())
                / (b.mean() + b.standard_deviation() * rng.next_gaussian())
        }))
    };
    let c = a / b;
    // The delta-method mean is first-order exact; allow a little bias slack.
    assert_close("div mean", em, c.mean(), 1e-2);
    assert_close("div variance", ev, c.variance(), 5e-2);
}

#[test]
fn monte_carlo_scalar_scaling() {
    let n = 500_000_u64;
    let a = Fuzzy::new(3.0_f64, 0.25);
    let k = 7.5_f64;
    let (em, ev) = {
        let mut rng = SplitMix(0x1234_5678);
        let scaled = (0..n).map(|_| (a.mean() + a.standard_deviation() * rng.next_gaussian()) * k);
        sample_moments(scaled)
    };
    let c = a * k;
    assert_close("scale mean", em, c.mean(), 1e-3);
    assert_close("scale variance", ev, c.variance(), 1e-2);
}

#[test]
fn monte_carlo_kalman_fusion() {
    // Empirical check that fusing two noisy readings yields the theoretical
    // minimum-variance combination.
    let n = 300_000_u64;
    let a = Fuzzy::new(10.0_f64, 1.0);
    let b = Fuzzy::new(12.0_f64, 3.0);
    let fused = a.fuse(&b);

    let mut rng = SplitMix(0x9E37_79B9);
    let (em, ev) = sample_moments((0..n).map(|_| {
        let ra = a.mean() + a.standard_deviation() * rng.next_gaussian();
        let rb = b.mean() + b.standard_deviation() * rng.next_gaussian();
        // Optimal two-measurement combination, m* = (m_a v_b + m_b v_a)/(v_a+v_b).
        (ra * b.variance() + rb * a.variance()) / (a.variance() + b.variance())
    }));
    assert_close("fusion mean", em, fused.mean(), 1e-3);
    assert_close("fusion variance", ev, fused.variance(), 2e-2);
}