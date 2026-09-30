//! [`Distribution<T>`]: a general probabilistic value.
//!
//! `Distribution<T>` is the more general uncertainty wrapper referenced in
//! `todo.md` Phase 1: it starts with the Gaussian case (mean + variance, the
//! same representation as [`crate::Fuzzy`]) and leaves room to grow other
//! variants (e.g. `Uniform`, `Categorical`) without breaking callers who
//! match on it exhaustively via the accessor methods below rather than the
//! enum shape directly.

use core::fmt;
use core::ops::{Add, Div, Mul, Sub};

use num_traits::Float;

use crate::Fuzzy;

/// A general probabilistic value.
///
/// Currently only the Gaussian and constant cases are implemented; other
/// distribution families are future work (see `todo.md`'s "AI & Probabilistic
/// Intelligence Foundation" section).
/// Not `Eq`: float-backed, so NaN would break reflexivity.
#[allow(clippy::derive_partial_eq_without_eq)] // deliberate: float backing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distribution<T> {
    /// A Gaussian (normal) distribution with the given mean and variance.
    Gaussian {
        /// Mean of the distribution.
        mean: T,
        /// Variance of the distribution.
        variance: T,
    },
    /// A deterministic (zero-variance) value.
    Constant(T),
}

/// The variance was negative or NaN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidVariance;

impl fmt::Display for InvalidVariance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("variance must be non-negative and finite")
    }
}

impl core::error::Error for InvalidVariance {}

impl<T: Float> Distribution<T> {
    /// Construct a Gaussian distribution from a mean and a variance.
    ///
    /// A zero variance is normalized to [`Self::Constant`] (the two are the
    /// same distribution, and keeping the degenerate `Gaussian { v: 0 }`
    /// shape around would split identical distributions into unequal
    /// variants). A NaN variance is treated as zero by IEEE comparison, so
    /// it is rejected here as well.
    ///
    /// # Panics
    /// Panics if `variance` is negative or NaN; use [`Self::try_gaussian`]
    /// for the fallible form.
    #[must_use]
    pub fn gaussian(mean: T, variance: T) -> Self {
        Self::try_gaussian(mean, variance).expect("variance must be non-negative")
    }

    /// Fallible [`Self::gaussian`].
    ///
    /// # Errors
    /// [`InvalidVariance`] when `variance` is negative or NaN.
    pub fn try_gaussian(mean: T, variance: T) -> Result<Self, InvalidVariance> {
        if variance.is_nan() || variance < T::zero() {
            return Err(InvalidVariance);
        }
        if variance == T::zero() {
            Ok(Self::Constant(mean))
        } else {
            Ok(Self::Gaussian { mean, variance })
        }
    }

    /// Construct a deterministic (certain) distribution.
    pub const fn constant(value: T) -> Self {
        Self::Constant(value)
    }

    /// The distribution's mean.
    pub const fn mean(&self) -> T {
        match self {
            Self::Gaussian { mean, .. } => *mean,
            Self::Constant(value) => *value,
        }
    }

    /// The distribution's variance.
    pub fn variance(&self) -> T {
        match self {
            Self::Gaussian { variance, .. } => *variance,
            Self::Constant(_) => T::zero(),
        }
    }

    /// The standard deviation (`sqrt(variance)`).
    pub fn standard_deviation(&self) -> T {
        self.variance().sqrt()
    }

    /// Draws a sample from the distribution using `rng`, a closure producing
    /// independent uniform draws in `[0, 1)`.
    ///
    /// Draws are clamped into the open interval `(0, 1)` before the quantile
    /// lookup — a literal `0.0` or `1.0` draw (which a buggy or edge-case RNG
    /// can produce) would otherwise panic inside
    /// [`crate::quants::norm_ppf`]; it now maps to the corresponding extreme
    /// quantile instead.
    ///
    /// # Panics
    /// Never: the draw clamp keeps `norm_ppf` inside its `(0, 1)` domain.
    #[must_use]
    pub fn sample(&self, mut rng: impl FnMut() -> f64) -> T
    where
        T: num_traits::FromPrimitive,
    {
        match self {
            Self::Constant(value) => *value,
            Self::Gaussian { mean, variance } => {
                let draw = rng().clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
                let z = T::from_f64(crate::quants::norm_ppf(draw)).unwrap();
                *mean + z * variance.sqrt()
            }
        }
    }

    /// Affine transform `a * x + b`, exact for the Gaussian case (means
    /// transform linearly, variances by `a^2`).
    #[must_use]
    pub fn affine(&self, a: T, b: T) -> Self {
        match *self {
            Self::Gaussian { mean, variance } => Self::Gaussian {
                mean: a * mean + b,
                variance: a * a * variance,
            },
            Self::Constant(value) => Self::Constant(a * value + b),
        }
    }

    /// Non-linear transform through `f`, using the first-order (delta-method)
    /// approximation with derivative `df` at the operating point: variance
    /// scales by `df^2`. For affine `f` prefer [`Distribution::affine`],
    /// which is exact.
    #[must_use]
    pub fn map_first_order(&self, f: impl Fn(T) -> T, df: T) -> Self {
        match *self {
            Self::Gaussian { mean, variance } => Self::Gaussian {
                mean: f(mean),
                variance: df * df * variance,
            },
            Self::Constant(value) => Self::Constant(f(value)),
        }
    }

    fn into_fuzzy(self) -> Fuzzy<T> {
        match self {
            Self::Gaussian { mean, variance } => Fuzzy::new(mean, variance),
            Self::Constant(value) => Fuzzy::constant(value),
        }
    }
}

impl<T: Float> From<Fuzzy<T>> for Distribution<T> {
    fn from(f: Fuzzy<T>) -> Self {
        if f.variance() == T::zero() {
            Self::Constant(f.mean())
        } else {
            Self::Gaussian {
                mean: f.mean(),
                variance: f.variance(),
            }
        }
    }
}

impl<T: Float> From<T> for Distribution<T> {
    fn from(v: T) -> Self {
        Self::Constant(v)
    }
}

/// Every [`Distribution`] *is* representable as a [`Fuzzy`]: a Gaussian maps
/// to the same mean/variance, a `Constant` to a zero-variance estimate — the
/// conversion is lossless, so it is an infallible `From` rather than a
/// `TryFrom` with an error case that could never carry information.
impl<T: Float> From<Distribution<T>> for Fuzzy<T> {
    fn from(d: Distribution<T>) -> Self {
        match d {
            Distribution::Gaussian { mean, variance } => Self::new(mean, variance),
            Distribution::Constant(value) => Self::constant(value),
        }
    }
}

macro_rules! impl_distribution_op {
    ($trait:ident, $method:ident) => {
        impl<T: Float> $trait for Distribution<T> {
            type Output = Distribution<T>;
            fn $method(self, rhs: Distribution<T>) -> Self::Output {
                self.into_fuzzy().$method(rhs.into_fuzzy()).into()
            }
        }
    };
}

impl_distribution_op!(Add, add);
impl_distribution_op!(Sub, sub);
impl_distribution_op!(Mul, mul);
impl_distribution_op!(Div, div);

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on analytic results
    #![allow(clippy::cast_precision_loss)] // deterministic seeded-RNG casts

    use super::*;

    #[test]
    fn moments() {
        let g = Distribution::<f64>::Gaussian {
            mean: 10.0,
            variance: 4.0,
        };
        assert_eq!(g.mean(), 10.0);
        assert_eq!(g.variance(), 4.0);
        assert_eq!(g.standard_deviation(), 2.0);
        let c = Distribution::Constant(3.0);
        assert_eq!(c.mean(), 3.0);
        assert_eq!(c.variance(), 0.0);
    }

    #[test]
    fn gaussian_arithmetic_matches_fuzzy() {
        let a: Distribution<f64> = Fuzzy::new(3.0, 0.25).into();
        let b: Distribution<f64> = Fuzzy::new(2.0, 0.5).into();
        let sum = a + b;
        let expected = Fuzzy::new(3.0, 0.25) + Fuzzy::new(2.0, 0.5);
        assert!((sum.mean() - expected.mean()).abs() < 1e-12);
        assert!((sum.variance() - expected.variance()).abs() < 1e-12);
        let prod = a * b;
        let expected = Fuzzy::new(3.0, 0.25) * Fuzzy::new(2.0, 0.5);
        assert!((prod.mean() - expected.mean()).abs() < 1e-12);
        assert!((prod.variance() - expected.variance()).abs() < 1e-12);
    }

    #[test]
    fn constant_arithmetic_is_exact() {
        let a = Distribution::Constant(5.0_f64);
        let b = Distribution::Constant(3.0_f64);
        assert_eq!((a + b), Distribution::Constant(8.0));
        assert_eq!((a - b), Distribution::Constant(2.0));
        assert_eq!((a * b), Distribution::Constant(15.0));
        assert_eq!((a / b), Distribution::Constant(5.0 / 3.0));
    }

    #[test]
    fn gaussian_plus_constant_keeps_variance() {
        let g = Distribution::<f64>::Gaussian {
            mean: 10.0,
            variance: 4.0,
        };
        let c = Distribution::Constant(1.0_f64);
        let sum = g + c;
        assert!((sum.mean() - 11.0).abs() < 1e-12);
        assert!((sum.variance() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn conversion_round_trips() {
        let f = Fuzzy::new(2.0_f64, 0.5);
        let d: Distribution<f64> = f.into();
        assert_eq!(d.mean(), 2.0);
        assert_eq!(d.variance(), 0.5);
        let back: Fuzzy<f64> = Fuzzy::from(d);
        assert!((back.mean() - 2.0).abs() < 1e-12);
        assert!((back.variance() - 0.5).abs() < 1e-12);

        let c = Distribution::Constant(1.0_f64);
        let back: Fuzzy<f64> = c.into();
        assert_eq!(back.variance(), 0.0);
    }

    #[test]
    fn sample_gaussian_matches_moments() {
        // Purely deterministic seeded RNG for reproducibility.
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut rng = || {
            state = state
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .rotate_left(37)
                .wrapping_add(1);
            (state >> 11) as f64 / (1_u64 << 53) as f64
        };
        let g = Distribution::<f64>::Gaussian {
            mean: 100.0,
            variance: 25.0,
        };
        let n = 400_000;
        let mut sum = 0.0;
        let mut sumsq = 0.0;
        for _ in 0..n {
            let x = g.sample(&mut rng);
            sum += x;
            sumsq += x * x;
        }
        let mean = sum / f64::from(n);
        let variance = sumsq / f64::from(n) - mean * mean;
        assert!((mean - 100.0).abs() < 0.05);
        assert!((variance - 25.0).abs() < 0.15);
    }

    #[test]
    fn affine_is_exact_for_gaussians() {
        let g = Distribution::<f64>::gaussian(10.0, 4.0);
        let t = g.affine(2.0, 1.0);
        assert_eq!(t.mean(), 21.0);
        assert_eq!(t.variance(), 16.0); // a^2 * v
        let c = Distribution::<f64>::Constant(3.0).affine(2.0, 1.0);
        assert_eq!(c.mean(), 7.0);
        assert_eq!(c.variance(), 0.0);
    }

    #[test]
    fn first_order_map_matches_affine_when_linear() {
        let g = Distribution::<f64>::gaussian(10.0, 4.0);
        let via_map = g.map_first_order(|x| 2.0 * x + 1.0, 2.0);
        let via_affine = g.affine(2.0, 1.0);
        assert_eq!(via_map.mean(), via_affine.mean());
        assert_eq!(via_map.variance(), via_affine.variance());
    }

    #[test]
    fn sample_constant_is_exact() {
        let c = Distribution::Constant(42.0_f64);
        for _ in 0..10 {
            assert_eq!(c.sample(|| 0.5), 42.0);
        }
    }

    #[test]
    fn zero_variance_gaussian_normalizes_to_constant() {
        let d = Distribution::<f64>::gaussian(5.0, 0.0);
        assert_eq!(d, Distribution::Constant(5.0));
        // Round trip: a Constant converts into a zero-variance Fuzzy.
        let f: Fuzzy<f64> = Distribution::Constant(7.0_f64).into();
        assert_eq!(f.mean(), 7.0);
        assert_eq!(f.variance(), 0.0);
    }

    #[test]
    fn negative_or_nan_variance_is_rejected() {
        assert_eq!(
            Distribution::<f64>::try_gaussian(1.0, -0.5),
            Err(InvalidVariance)
        );
        assert!(Distribution::try_gaussian(1.0, f64::NAN).is_err());
    }

    #[test]
    #[should_panic(expected = "variance must be non-negative")]
    fn gaussian_panics_on_negative_variance() {
        let _ = Distribution::<f64>::gaussian(1.0, -1.0);
    }

    #[test]
    fn sample_survives_degenerate_rng_draws() {
        // A literal 0.0 or 1.0 draw is clamped, not a panic.
        let g = Distribution::<f64>::gaussian(100.0, 25.0);
        let lo = g.sample(|| 0.0);
        let hi = g.sample(|| 1.0);
        assert!(lo.is_finite() && hi.is_finite());
        assert!(lo < 100.0 && hi > 100.0);
    }
}
