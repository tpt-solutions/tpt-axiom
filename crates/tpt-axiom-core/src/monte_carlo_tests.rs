//! Monte Carlo cross-checks: sample many independent draws and compare the
//! empirical mean/variance against the analytically propagated values from
//! [`crate::Fuzzy`]'s operator overloads.

use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::Fuzzy;

/// Draw a standard-normal sample via the Box-Muller transform.
fn standard_normal(rng: &mut StdRng) -> f64 {
    let u1: f64 = rng.gen_range(f64::EPSILON..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn sample(fuzzy: Fuzzy<f64>, rng: &mut StdRng) -> f64 {
    fuzzy.std_dev().mul_add(standard_normal(rng), fuzzy.mean())
}

#[allow(clippy::cast_precision_loss)]
fn empirical_mean_variance(samples: &[f64]) -> (f64, f64) {
    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, variance)
}

/// `actual` is within `rel_tol` of `expected`, relative to `expected`'s
/// magnitude (with a floor of `1.0` so cross-checks near zero don't demand
/// unreasonable absolute precision).
fn within_relative_tolerance(actual: f64, expected: f64, rel_tol: f64) -> bool {
    (actual - expected).abs() <= rel_tol * expected.abs().max(1.0)
}

const SAMPLES: usize = 200_000;

#[test]
fn monte_carlo_cross_check_sum_variance() {
    let mut rng = StdRng::seed_from_u64(42);
    let a = Fuzzy::new(2.0, 0.5);
    let b = Fuzzy::new(3.0, 0.25);

    let sums: Vec<f64> = (0..SAMPLES)
        .map(|_| sample(a, &mut rng) + sample(b, &mut rng))
        .collect();
    let (empirical_mean, empirical_variance) = empirical_mean_variance(&sums);

    let analytic = a + b;
    assert!(within_relative_tolerance(
        empirical_mean,
        analytic.mean(),
        0.05
    ));
    assert!(within_relative_tolerance(
        empirical_variance,
        analytic.variance(),
        0.1
    ));
}

#[test]
fn monte_carlo_cross_check_product_variance() {
    let mut rng = StdRng::seed_from_u64(7);
    let a = Fuzzy::new(2.0, 0.5);
    let b = Fuzzy::new(3.0, 0.25);

    let products: Vec<f64> = (0..SAMPLES)
        .map(|_| sample(a, &mut rng) * sample(b, &mut rng))
        .collect();
    let (empirical_mean, empirical_variance) = empirical_mean_variance(&products);

    let analytic = a * b;
    assert!(within_relative_tolerance(
        empirical_mean,
        analytic.mean(),
        0.05
    ));
    assert!(within_relative_tolerance(
        empirical_variance,
        analytic.variance(),
        0.15
    ));
}

#[test]
fn monte_carlo_cross_check_scalar_scaling_variance() {
    let mut rng = StdRng::seed_from_u64(123);
    let a = Fuzzy::new(4.0, 1.2);
    let c = 2.5;

    let scaled: Vec<f64> = (0..SAMPLES).map(|_| sample(a, &mut rng) * c).collect();
    let (empirical_mean, empirical_variance) = empirical_mean_variance(&scaled);

    let analytic = a * c;
    assert!(within_relative_tolerance(
        empirical_mean,
        analytic.mean(),
        0.05
    ));
    assert!(within_relative_tolerance(
        empirical_variance,
        analytic.variance(),
        0.1
    ));
}
