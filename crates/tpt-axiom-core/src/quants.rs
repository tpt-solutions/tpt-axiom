//! Small, dependency-free approximations of the special functions used by the
//! statistical helpers: the error function, the standard-normal CDF, and the
//! standard-normal quantile function (inverse CDF).
//!
//! All functions operate on `f64`. [`crate::Fuzzy<T>`] converts through these
//! in its generic [`num_traits::Float`] implementation, so accuracy is bounded
//! by the approximations below (relative error ~1e-7 or better).

const SQRT_2: f64 = 1.414_213_562_373_095_1;

/// Error function `erf(x)`, computed with the Abramowitz & Stegun 7.1.26
/// rational approximation (absolute error ~1.5e-7).
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    if x >= 6.0 {
        return sign;
    }
    let p = 0.327_591_1;
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
        + 0.254_829_592)
        * t
        * (-(x * x)).exp();
    sign * y
}

/// Standard-normal cumulative density function `P(Z <= z)`.
pub fn norm_cdf(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    if !z.is_finite() {
        return if z > 0.0 { 1.0 } else { 0.0 };
    }
    if z >= 0.0 {
        0.5 * (1.0 + erf(z / SQRT_2))
    } else {
        1.0 - norm_cdf(-z)
    }
}

/// Standard-normal quantile function (inverse CDF) `z_p`, approximated with
/// the Acklam / Beasley-Springer-Moro rational approximation (relative error
/// ~1.15e-9, tighter than [`norm_cdf`]'s ~1.5e-7 absolute error, so it is
/// intentionally *not* refined by Newton iterations against `norm_cdf`: doing
/// so would anchor the result to the less accurate function instead of
/// improving it).
///
/// # Panics
///
/// Panics if `p` is outside the open interval `(0, 1)`.
pub fn norm_ppf(p: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) {
        panic!("norm_ppf requires 0 < p < 1, got {p}");
    }

    const A: [f64; 8] = [
        3.387_132_872_796_366_6,
        133.141_667_891_784_38,
        1_971.590_950_306_551_4,
        13_731.693_765_509_461,
        45_921.953_931_549_87,
        67_265.770_927_008_7,
        33_430.575_583_588_126,
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
        5_226.495_278_852_854_5,
    ];
    const C: [f64; 8] = [
        1.423_437_110_749_683_6,
        4.630_337_846_156_545,
        5.769_497_221_460_692,
        3.647_848_324_763_204_3,
        1.270_458_252_452_368_4,
        0.241_780_725_177_450_6,
        0.022_723_844_989_269_182,
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
    if q.abs() <= 0.425 {
        let r = 0.180_625 - q * q;
        q * horner(&A, r) / horner(&B, r)
    } else {
        let r = if q < 0.0 { p } else { 1.0 - p };
        let r = (-r.ln()).sqrt() - 1.6;
        let x = horner(&C, r) / horner(&D, r);
        if q < 0.0 { -x } else { x }
    }
}

fn horner(coeffs: &[f64; 8], x: f64) -> f64 {
    coeffs
        .iter()
        .rev()
        .fold(0.0, |acc, &c| acc * x + c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_known_values() {
        assert!((erf(0.0) - 0.0).abs() < 1e-7);
        assert!((erf(1.0) - 0.842_700_792_949_714_9).abs() < 1e-6);
        assert!((erf(-1.0) + 0.842_700_792_949_714_9).abs() < 1e-6);
        assert!((erf(2.0) - 0.995_322_265_018_952_7).abs() < 1e-6);
    }

    #[test]
    fn norm_cdf_known_values() {
        assert!((norm_cdf(0.0) - 0.5).abs() < 1e-9);
        assert!((norm_cdf(1.96) - 0.975).abs() < 1e-4);
        assert!((norm_cdf(-1.96) - 0.025).abs() < 1e-4);
        assert!((norm_cdf(6.0) - 1.0).abs() < 1e-9);
        assert!((norm_cdf(-6.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn norm_ppf_inverts_cdf() {
        assert!((norm_ppf(0.975) - 1.959_963_984_540_054).abs() < 1e-6);
        assert!((norm_ppf(0.025) + 1.959_963_984_540_054).abs() < 1e-6);
        assert!((norm_ppf(0.5) - 0.0).abs() < 1e-12);
        // Round-trip, bounded by norm_cdf's own ~1.5e-7 absolute error (it
        // goes through the erf approximation), not norm_ppf's ~1.15e-9.
        for p in [0.05, 0.10, 0.5, 0.9, 0.95, 0.99] {
            let z = norm_ppf(p);
            assert!((norm_cdf(z) - p).abs() < 1e-6, "round trip failed for {p}");
        }
    }

    #[test]
    #[should_panic]
    fn norm_ppf_rejects_out_of_range() {
        let _ = norm_ppf(0.0);
    }

    #[test]
    #[should_panic]
    fn norm_ppf_rejects_upper_bound() {
        let _ = norm_ppf(1.0);
    }
}
