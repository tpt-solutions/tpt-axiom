//! [`Distribution<T>`]: a general probabilistic value.
//!
//! `Distribution<T>` is the more general uncertainty wrapper referenced in
//! `todo.md` Phase 1: it starts with the Gaussian case (mean + variance, the
//! same representation as [`crate::Fuzzy`]) and leaves room to grow other
//! variants (e.g. `Uniform`, `Categorical`) without breaking callers who
//! match on it exhaustively via the accessor methods below rather than the
//! enum shape directly.

use crate::{Fuzzy, traits::FuzzyScalar};

/// A general probabilistic value. Currently only the Gaussian case is
/// implemented; other distribution families are future work (see
/// `todo.md`'s "AI & Probabilistic Intelligence Foundation" section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution<T> {
    /// A Gaussian (normal) distribution with the given mean and variance.
    Gaussian {
        /// Mean of the distribution.
        mean: T,
        /// Variance of the distribution.
        variance: T,
    },
}

impl<T: FuzzyScalar> Distribution<T> {
    /// Construct a Gaussian distribution from a mean and a variance.
    pub const fn gaussian(mean: T, variance: T) -> Self {
        Self::Gaussian { mean, variance }
    }

    /// The distribution's mean.
    pub const fn mean(&self) -> T {
        match self {
            Self::Gaussian { mean, .. } => *mean,
        }
    }

    /// The distribution's variance.
    pub const fn variance(&self) -> T {
        match self {
            Self::Gaussian { variance, .. } => *variance,
        }
    }
}

impl<T: Float> From<Fuzzy<T>> for Distribution<T> {
    fn from(f: Fuzzy<T>) -> Self {
        Distribution::Gaussian {
            mean: f.mean(),
            variance: f.variance(),
        }
    }
}

impl<T: Float> From<T> for Distribution<T> {
    fn from(v: T) -> Self {
        Distribution::Constant(v)
    }
}

/// Error returned when converting a non-Gaussian [`Distribution`] into a
/// [`Fuzzy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotGaussian;

impl fmt::Display for NotGaussian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "distribution is not Gaussian (or constant)")
    }
}

impl<T: Float> TryFrom<Distribution<T>> for Fuzzy<T> {
    type Error = NotGaussian;
    fn try_from(d: Distribution<T>) -> Result<Self, Self::Error> {
        match d {
            Distribution::Gaussian { mean, variance } => Ok(Fuzzy::new(mean, variance)),
            Distribution::Constant(_) => Err(NotGaussian),
        }
    }
}

macro_rules! impl_distribution_op {
    ($trait:ident, $method:ident) => {
        impl<T: Float> $trait for Distribution<T> {
            type Output = Distribution<T>;
            fn $method(self, rhs: Distribution<T>) -> Self::Output {
                (self.into_fuzzy().$method(rhs.into_fuzzy())).into()
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
        let back: Fuzzy<f64> = d.try_into().unwrap();
        assert!((back.mean() - 2.0).abs() < 1e-12);
        assert!((back.variance() - 0.5).abs() < 1e-12);

        let c = Distribution::Constant(1.0_f64);
        assert!(Fuzzy::<f64>::try_from(c).is_err());
    }

    #[test]
    fn sample_gaussian_matches_moments() {
        // Purely deterministic seeded RNG for reproducibility.
        let mut state = 0x2545F4914F6CDD1D_u64;
        let mut rng = || {
            state = state.wrapping_mul(0x9E3779B97F4A7C15).rotate_left(37).wrapping_add(1);
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
        let mean = sum / n as f64;
        let variance = sumsq / n as f64 - mean * mean;
        assert!((mean - 100.0).abs() < 0.05);
        assert!((variance - 25.0).abs() < 0.15);
    }

    #[test]
    fn sample_constant_is_exact() {
        let c = Distribution::Constant(42.0_f64);
        for _ in 0..10 {
            assert_eq!(c.sample(|| 0.5), 42.0);
        }
    }
}
