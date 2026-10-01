//! Small, dependency-free implementations of the special functions used by
//! the statistical helpers: the error function, the complementary error
//! function, the standard-normal CDF, and the standard-normal quantile
//! function (inverse CDF).
//!
//! All functions operate on `f64` and are accurate to near machine precision
//! (~1e-15 relative):
//!
//! * [`erf`] / [`erfc`] use a power series for small arguments and the
//!   continued fraction (Abramowitz & Stegun 7.1.14) for large ones —
//!   unlike the classic rational approximations this keeps *relative*
//!   accuracy in the far tail, which is what makes the refined quantile
//!   below trustworthy.
//! * [`norm_cdf`] is `erfc(-z/√2)/2`.
//! * [`norm_ppf`] starts from the Acklam rational approximation
//!   (relative error ~1.15e-9) and polishes with Newton iterations against
//!   the log-CDF, which stays stable for quantiles far into the tails,
//!   where `Φ(z) - p` would cancel to nothing.

use core::f64::consts::{PI, SQRT_2};

// In the `libm` (no_std) build the float methods come from the
// `num_traits::Float` trait, not inherent `std` methods.
#[cfg(all(not(feature = "std"), feature = "libm"))]
use num_traits::Float;

/// Error function `erf(x)`, full `f64` precision (see [`erfc`]).
#[must_use]
pub fn erf(x: f64) -> f64 {
    1.0 - erfc(x)
}

/// Complementary error function `erfc(x) = 1 - erf(x)`.
///
/// Accurate to near machine precision over the whole line, including the
/// far tail (relative — not just absolute — accuracy is preserved down to
/// the smallest representable values).
#[must_use]
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x.is_infinite() {
        return if x > 0.0 { 0.0 } else { 2.0 };
    }
    if x >= 0.0 {
        erfc_nonneg(x)
    } else {
        2.0 - erfc_nonneg(-x)
    }
}

fn erfc_nonneg(x: f64) -> f64 {
    if x < 2.0 {
        // Power series for erf: `erf(x) = 2/√π Σ (-1)^n x^(2n+1)/(n!(2n+1))`.
        // At x = 2 the sum is ~0.995, so the final `1 -` subtraction costs at
        // most ~2.3 digits — comfortably inside double precision.
        1.0 - erf_series(x)
    } else {
        erfc_continued_fraction(x)
    }
}

/// The Maclaurin series of `erf`, summed until the terms fall below the
/// rounding noise of the partial sum (≤ ~25 terms at the x < 2 cutoff).
fn erf_series(x: f64) -> f64 {
    let x2 = x * x;
    let mut term = x;
    let mut sum = x;
    let mut n: u32 = 1;
    while term.abs() > 1e-18 * sum.abs() && n < 200 {
        // term_n = term_{n-1} * (-(x²)(2n-1)) / (n(2n+1))
        let nf = f64::from(n);
        term *= -x2 * (2.0 * nf - 1.0) / (nf * (2.0 * nf + 1.0));
        sum += term;
        n += 1;
    }
    sum * (2.0 / PI.sqrt())
}

/// Continued fraction (A&S 7.1.14) for the upper tail:
/// `erfc(x) = exp(-x²)/√π · 1/(x + (1/2)/(x + (2/2)/(x + …)))`, evaluated
/// with the modified Lentz algorithm.
fn erfc_continued_fraction(x: f64) -> f64 {
    // `f`/`c`/`d` are the standard A&S Lentz variables.
    #![allow(clippy::many_single_char_names)]
    const TINY: f64 = 1e-300;
    let mut f = x.max(TINY);
    // Modified Lentz: C starts at b0 (not infinity — that would make the
    // first delta exactly 1 and fake convergence in one step).
    let mut c = f;
    let mut d = 0.0;
    for i in 1..=300u32 {
        let a = f64::from(i) / 2.0;
        d = x + a * d;
        if d == 0.0 {
            d = TINY;
        }
        c = x + a / c;
        if c == 0.0 {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < 1e-17 {
            break;
        }
    }
    (-x * x).exp() / (PI.sqrt() * f)
}

/// Standard-normal cumulative density function `P(Z <= z)`, via the
/// full-precision [`erfc`].
#[must_use]
pub fn norm_cdf(z: f64) -> f64 {
    erfc(-z / SQRT_2) / 2.0
}

/// Acklam's rational approximation of the standard-normal quantile.
///
/// Relative error ~1.15e-9 — the initial estimate [`norm_ppf`] polishes
/// against the full-precision CDF.
#[allow(clippy::excessive_precision)] // published Acklam constants, digit for digit
fn acklam_ppf(p: f64) -> f64 {
    const A: [f64; 8] = [
        3.387_132_872_796_366_6,
        133.141_667_891_784_38,
        1_971.590_950_306_551_4,
        13_731.693_765_509_46,
        45_921.953_931_549_87,
        67_265.770_927_008_7,
        33_430.575_583_588_13,
        2_509.080_928_730_122_5,
    ];
    const B: [f64; 8] = [
        1.0,
        42.313_330_701_600_91,
        687.187_007_492_058,
        5_394.196_021_424_751,
        21_213.794_301_586_596,
        39_307.895_800_092_71,
        28_729.085_735_721_942,
        5_226.495_278_852_854,
    ];
    const C: [f64; 8] = [
        1.423_437_110_749_683_6,
        4.630_337_846_156_545,
        5.769_497_221_460_692,
        3.647_848_324_763_204,
        1.270_458_252_452_368_4,
        0.241_780_725_177_450_6,
        0.022_723_844_989_269_18,
        0.000_774_545_014_278_341_5,
    ];
    const D: [f64; 8] = [
        1.0,
        2.053_191_626_637_759,
        1.676_384_830_183_804,
        0.689_767_334_985_1,
        0.148_103_976_427_480_1,
        0.015_198_666_563_616_4,
        0.000_547_593_808_499_534_6,
        0.000_000_001_050_750_071_644_417,
    ];

    let q = p - 0.5;
    #[allow(clippy::float_cmp)] // the branch point is part of the published approximation
    if q.abs() <= 0.425 {
        let r = 0.180_625 - q * q;
        return q * horner(&A, r) / horner(&B, r);
    }
    let r = if q < 0.0 { p } else { 1.0 - p };
    let r = (-r.ln()).sqrt() - 1.6;
    let x = horner(&C, r) / horner(&D, r);
    if q < 0.0 { -x } else { x }
}

fn horner(coeffs: &[f64; 8], x: f64) -> f64 {
    coeffs.iter().rev().fold(0.0, |acc, &c| acc * x + c)
}

/// Standard-normal quantile function (inverse CDF) `z_p`.
///
/// Acklam's rational approximation provides the start (relative error
/// ~1.15e-9); Newton iterations in *log space* against the full-precision
/// [`norm_cdf`] then refine it to near machine precision. Log space is what
/// keeps the far tail honest: for `p = 1e-300`, `Φ(z) - p` is pure
/// cancellation in linear space, while `ln Φ(z) - ln p` stays meaningful
/// all the way down.
///
/// # Panics
///
/// Panics if `p` is outside the open interval `(0, 1)`.
#[must_use]
pub fn norm_ppf(p: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0, "norm_ppf requires 0 < p < 1, got {p}");
    let p = p.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);

    // Solve the lower tail q = min(p, 1-p) with a negative z, then mirror.
    // (Computing the start from `1 - q` would cancel to 1.0 in the far
    // tail; `acklam_ppf(q)` takes its own tail branch on `ln q` instead.)
    let (lower, q) = if p < 0.5 { (true, p) } else { (false, 1.0 - p) };
    let log_target = q.ln();
    let mut z = acklam_ppf(q); // negative for q < 0.5, zero at q = 0.5
    let log_sqrt_2pi = 0.5 * (2.0 * PI).ln();
    for _ in 0..4 {
        // ln Φ(z), ln φ(z) for z <= 0 — no cancellation, no underflow.
        let log_phi = erfc(-z / SQRT_2).ln() - core::f64::consts::LN_2;
        let log_density = -z * z / 2.0 - log_sqrt_2pi;
        let error = log_phi - log_target;
        #[allow(clippy::float_cmp)] // convergence threshold, not an equality test
        if error.abs() < 1e-16 {
            break;
        }
        // Newton step for L(z) = ln Φ(z) - ln q; L'(z) = φ(z)/Φ(z) = e^(ln φ - ln Φ).
        let slope = (log_density - log_phi).exp();
        let mut next = z - error / slope;
        if next > 0.0 {
            next = z / 2.0; // keep the iteration on the negative side
        }
        z = next;
    }
    if lower { z } else { -z }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_known_values() {
        assert!(erf(0.0).abs() < 1e-15);
        assert!((erf(1.0) - 0.842_700_792_949_714_9).abs() < 1e-15);
        assert!((erf(-1.0) + 0.842_700_792_949_714_9).abs() < 1e-15);
        assert!((erf(2.0) - 0.995_322_265_018_952_7).abs() < 1e-14);
        assert!((erf(4.0) - 0.999_999_984_582_742_1).abs() < 1e-15);
        assert!((erf(6.0) - 1.0).abs() < 1e-15);
    }

    #[test]
    #[allow(clippy::excessive_precision, clippy::float_cmp)] // published reference values
    fn erfc_tail_stays_relative_accurate() {
        // Deep-tail values: relative accuracy, not just absolute.
        // erfc(4)  = 1.5417257900280020e-8
        // erfc(6)  = 2.1519736712498913e-17
        // erfc(10) = 2.0884875837625448e-45
        assert!((erfc(4.0) / 1.541_725_790_028_002e-8 - 1.0).abs() < 1e-13);
        assert!((erfc(6.0) / 2.151_973_671_249_891_3e-17 - 1.0).abs() < 1e-12);
        assert!((erfc(10.0) / 2.088_487_583_762_544_8e-45 - 1.0).abs() < 1e-11);
        assert_eq!(erfc(f64::INFINITY), 0.0);
        assert_eq!(erfc(f64::NEG_INFINITY), 2.0);
    }

    #[test]
    fn erf_and_erfc_are_complementary() {
        for x in [-3.5, -1.0, -0.3, 0.0, 0.7, 1.9, 3.0, 7.5] {
            assert!(
                (erf(x) + erfc(x) - 1.0).abs() < 1e-14,
                "complementarity failed at {x}"
            );
        }
    }

    #[test]
    fn norm_cdf_known_values() {
        assert!((norm_cdf(0.0) - 0.5).abs() < 1e-15);
        assert!((norm_cdf(1.96) - 0.975_002_104_851_779_5).abs() < 1e-14);
        assert!((norm_cdf(-1.96) - 0.024_997_895_148_220_435).abs() < 1e-14);
        // Φ(6) = 1 - 9.865876450376946e-10, preserved to full precision.
        assert!((norm_cdf(6.0) - 0.999_999_999_013_412_3).abs() < 1e-15);
        // The far tail keeps *relative* accuracy (the old A&S-based cdf
        // collapsed to 0 or 1 here).
        let tail = norm_cdf(-8.0);
        assert!(
            (tail / 6.220_960_574_271_78e-16 - 1.0).abs() < 1e-12,
            "{tail}"
        );
    }

    #[test]
    fn norm_ppf_inverts_cdf_to_machine_precision() {
        assert!((norm_ppf(0.975) - 1.959_963_984_540_054).abs() < 1e-12);
        assert!((norm_ppf(0.025) + 1.959_963_984_540_054).abs() < 1e-12);
        assert!(norm_ppf(0.5).abs() < 1e-14);
        for p in [0.05, 0.10, 0.5, 0.9, 0.95, 0.99, 0.999, 0.999_999_9] {
            let z = norm_ppf(p);
            let back = norm_cdf(z);
            assert!(
                (back - p).abs() < 1e-14 * p.max(1e-12),
                "round trip failed for {p}: got {back}"
            );
        }
    }

    #[test]
    fn norm_ppf_far_tail_round_trips() {
        // The whole point of the log-space refinement: quantiles the old
        // implementation could not reach.
        for p in [1e-10, 1e-20, 1e-50, 1e-100, 1e-200, 1e-300] {
            let z = norm_ppf(p);
            let back = norm_cdf(z);
            let relative = ((back - p) / p).abs();
            assert!(
                relative < 1e-10,
                "far tail failed for {p}: z={z}, back={back}"
            );
        }
        // 1 - 1e-20 rounds to 1.0 in f64, so the smallest representable
        // upper tail is one epsilon below one.
        for p in [1.0 - 1e-10, 1.0 - f64::EPSILON] {
            let z = norm_ppf(p);
            assert!(z > 0.0);
            assert!((norm_cdf(z) - p).abs() < 1e-14, "upper tail failed for {p}");
        }
    }

    #[test]
    #[should_panic(expected = "norm_ppf requires 0 < p < 1")]
    fn norm_ppf_rejects_out_of_range() {
        let _ = norm_ppf(0.0);
    }

    #[test]
    #[should_panic(expected = "norm_ppf requires 0 < p < 1")]
    fn norm_ppf_rejects_upper_bound() {
        let _ = norm_ppf(1.0);
    }
}
