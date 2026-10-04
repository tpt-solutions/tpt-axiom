//! Dependency-free special functions backing the distribution families:
//! the log-gamma function, the regularized incomplete gamma function, and
//! the regularized incomplete beta function.
//!
//! Accuracy notes:
//!
//! * `lgamma` uses the Lanczos approximation (g = 7, n = 9 coefficients),
//!   good to ~1e-15 relative for positive arguments.
//! * `gamma_p` / `gamma_q` (regularized lower/upper incomplete gamma) use a
//!   series for `x < a + 1` and a continued fraction (Lentz) otherwise — the
//!   classic Numerical Recipes split — accurate to ~1e-14 in their domains.
//! * `beta_reg` (regularized incomplete beta) uses the mirror symmetry
//!   `I_x(a, b) = 1 − I_{1−x}(b, a)` plus the standard continued fraction,
//!   which converges fast for `x < (a + 2)/(2a + b + 2)` — the same shape NR
//!   uses.

#![allow(clippy::excessive_precision)] // published Lanczos coefficients, digit for digit

// Without std, the f64 float methods come from num-traits (libm).
use core::f64::consts::PI;
#[cfg(not(feature = "std"))]
use num_traits::Float as _;

/// Lanczos coefficients for g = 7, n = 9.
const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_93,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_13,
    -176.615_029_162_140_59,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_571_6e-6,
    1.505_632_735_149_311_6e-7,
];

/// Natural log of the gamma function for positive arguments.
#[must_use]
pub fn lgamma(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return f64::NAN;
    }
    if x < 0.5 {
        // Reflection: Γ(x)Γ(1−x) = π / sin(πx).
        let r = PI / (PI * x).sin().abs();
        r.ln() - lgamma(1.0 - x)
    } else {
        let z = x - 1.0;
        let mut a = LANCZOS[0];
        let t = z + 7.5;
        for (i, &c) in LANCZOS.iter().enumerate().skip(1) {
            #[allow(clippy::cast_precision_loss)] // loop counter, i <= 8
            let fi = i as f64;
            a += c / (z + fi);
        }
        // Assembled directly in log space: computing Γ first would overflow
        // already around x ≈ 172.
        0.5 * (2.0 * PI).ln() + (z + 0.5) * t.ln() - t + a.ln()
    }
}

/// `Γ(x)` for positive arguments (exponentiated [`lgamma`]).
#[must_use]
pub fn gamma(x: f64) -> f64 {
    lgamma(x).exp()
}

/// The continued-fraction tail of the regularized incomplete gamma
/// (`Numerical Recipes`' `gcf`), computing `Q(a, x)` via modified Lentz.
fn gamma_q_cf(a: f64, x: f64) -> f64 {
    // `b`/`c`/`d`/`h` are the standard Numerical Recipes Lentz variables.
    #![allow(clippy::many_single_char_names)]
    const TINY: f64 = 1e-300;
    const MAX_ITER: u32 = 500;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=MAX_ITER {
        let fi = f64::from(i);
        let numerator = -fi * (fi - a);
        b += 2.0;
        d = numerator * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + numerator / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-15 {
            break;
        }
    }
    (-x + a * x.ln() - lgamma(a)).exp() * h
}

/// The series for the regularized lower incomplete gamma (`gser`).
fn gamma_p_series(a: f64, x: f64) -> f64 {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut delta = sum;
    for _ in 0..500 {
        ap += 1.0;
        delta *= x / ap;
        sum += delta;
        if delta.abs() < delta.abs() * 1e-15 {
            break;
        }
        if delta == 0.0 {
            break;
        }
    }
    sum * (-x + a * x.ln() - lgamma(a)).exp()
}

/// Regularized lower incomplete gamma `P(a, x) = γ(a, x) / Γ(a)`.
///
/// `a` must be positive; `x ≥ 0`.
#[must_use]
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if a <= 0.0 || x < 0.0 || a.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x.is_infinite() {
        return 1.0;
    }
    if x < a + 1.0 {
        gamma_p_series(a, x).clamp(0.0, 1.0)
    } else {
        1.0 - gamma_q_cf(a, x)
    }
}

/// Regularized upper incomplete gamma `Q(a, x) = Γ(a, x) / Γ(a) = 1 − P(a, x)`,
/// computed directly (no cancellation for the far upper tail).
#[must_use]
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if a <= 0.0 || x < 0.0 || a.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    if x.is_infinite() {
        return 0.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_p_series(a, x)
    } else {
        gamma_q_cf(a, x)
    }
}

/// The continued fraction of the regularized incomplete beta
/// (`Numerical Recipes`' `betacf`), via modified Lentz.
fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    // `a`/`b`/`x` are the function's own parameters, `c`/`d`/`h`/`m` are the
    // standard Numerical Recipes Lentz variables.
    #![allow(clippy::many_single_char_names)]
    const TINY: f64 = 1e-300;
    const MAX_ITER: u32 = 500;
    let sum_ab = a + b;
    let a_plus = a + 1.0;
    let a_minus = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - sum_ab * x / a_plus;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITER {
        let mf = f64::from(m);
        let m2 = 2.0 * mf;
        let mut aa = mf * (b - mf) * x / ((a_minus + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + mf) * (sum_ab + mf) * x / ((a + m2) * (a_plus + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-15 {
            break;
        }
    }
    h
}

/// Regularized incomplete beta `I_x(a, b)`.
///
/// `a`, `b` must be positive, `x` in `[0, 1]`.
#[must_use]
pub fn beta_reg(a: f64, b: f64, x: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 || a.is_nan() || b.is_nan() || !(0.0..=1.0).contains(&x) {
        return f64::NAN;
    }
    // `I_x(a, b)` is exactly `x` at both endpoints; the endpoints arrive here
    // as literal 0.0 / 1.0 from the caller, so an exact comparison is correct
    // (and any other value would be a domain error handled above).
    #[allow(clippy::float_cmp)]
    if x == 0.0 || x == 1.0 {
        return x;
    }
    // Front factor: x^a (1−x)^b / (a·B(a, b)).
    let ln_front = a * x.ln() + b * (1.0 - x).ln() - lgamma(a) - lgamma(b) + lgamma(a + b);
    let front = ln_front.exp();
    // Switch to the mirror form where the continued fraction converges fast.
    if x < (a + 2.0) / (a + b + 2.0) {
        front * beta_cf(a, b, x) / a
    } else {
        1.0 - front * beta_cf(b, a, 1.0 - x) / b
    }
}

/// `ln Γ(a) + ln Γ(b) − ln Γ(a + b)` — the log beta function.
#[must_use]
pub fn ln_beta(a: f64, b: f64) -> f64 {
    lgamma(a) + lgamma(b) - lgamma(a + b)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn lgamma_matches_known_values() {
        // ln Γ(1) = 0, ln Γ(2) = 0, ln Γ(5) = ln 24, ln Γ(0.5) = ln √π.
        assert!(lgamma(1.0).abs() < 1e-13);
        assert!((lgamma(2.0)).abs() < 1e-12);
        assert!((lgamma(5.0) - 24.0_f64.ln()).abs() < 1e-12);
        assert!((lgamma(0.5) - (PI.sqrt()).ln()).abs() < 1e-13);
        assert!((lgamma(10.0) - 362_880.0_f64.ln()).abs() < 1e-10);
        // Large arguments must not overflow (Γ first would overflow ~x=172).
        assert!((lgamma(1002.0) - 5_919.036_933_267_479).abs() < 1e-9);
        assert!(lgamma(1.0e5).is_finite());
        assert!(lgamma(0.0).is_nan());
    }

    #[test]
    fn incomplete_gamma_known_values() {
        // Exponential connection: P(1, x) = 1 − e^{−x}.
        for x in [0.5, 1.0, 2.5, 10.0] {
            assert!(
                (gamma_p(1.0, x) - (1.0 - (-x).exp())).abs() < 1e-13,
                "P(1, {x})"
            );
        }
        // Erlang: P(2, x) = 1 − e^{−x}(1 + x).
        for x in [0.5, 3.0, 8.0] {
            assert!(
                (gamma_p(2.0, x) - (1.0 - (-x).exp() * (1.0 + x))).abs() < 1e-13,
                "P(2, {x})"
            );
        }
        // Bounds and tails.
        assert_eq!(gamma_p(2.0, 0.0), 0.0);
        assert_eq!(gamma_p(2.0, f64::INFINITY), 1.0);
        assert!(gamma_q(3.0, 100.0) < 1e-30, "upper tail stays relative");
    }

    #[test]
    fn incomplete_beta_known_values() {
        // Beta(a, 1): I_x(a, 1) = x^a.
        for a in [0.5, 2.0, 5.5] {
            for x in [0.1, 0.5, 0.9] {
                assert!(
                    (beta_reg(a, 1.0, x) - x.powf(a)).abs() < 1e-13,
                    "I_{x}({a}, 1)"
                );
            }
        }
        // Symmetry: I_x(a, b) = 1 − I_{1−x}(b, a).
        for (a, b) in [(2.0, 3.0), (0.5, 0.5), (8.0, 2.0)] {
            let x = 0.3;
            assert!(
                (beta_reg(a, b, x) - (1.0 - beta_reg(b, a, 1.0 - x))).abs() < 1e-13,
                "symmetry at ({a}, {b})"
            );
        }
        // Uniform: I_x(1, 1) = x.
        assert!((beta_reg(1.0, 1.0, 0.42) - 0.42).abs() < 1e-14);
    }
}
