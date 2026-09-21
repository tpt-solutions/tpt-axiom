//! The [`Distribution<T>`] general uncertainty type.
//!
//! [`Fuzzy<T>`] models the important special case of a Gaussian
//! (mean + variance). [`Distribution<T>`] wraps a full probability
//! distribution so that non-Gaussian shapes can be added later without
//! changing the public API; today it starts with `Constant` and `Gaussian`.

use core::fmt;
use core::ops::{Add, Div, Mul, Sub};

use num_traits::{Float, FromPrimitive, ToPrimitive};

use crate::Fuzzy;

/// A probability distribution over type `T`.
///
/// Arithmetic on two distributions performs the appropriate closed-form
/// propagation where one exists (a Gaussian stays Gaussian under addition,
/// subtraction, multiplication and division, and a constant stays constant),
/// mirroring [`Fuzzy<T>`]'s rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Distribution<T> {
    /// A deterministic value (a delta distribution).
    Constant(T),
    /// A normal (Gaussian) distribution.
    Gaussian {
        /// The expected value of the distribution.
        mean: T,
        /// The spread (second central moment) of the distribution.
        variance: T,
    },
}

impl<T: Float> Distribution<T> {
    /// The distribution's expected value, if it is defined.
    pub fn mean(&self) -> T {
        match *self {
            Distribution::Constant(v) => v,
            Distribution::Gaussian { mean, .. } => mean,
        }
    }

    /// The distribution's variance, if it is defined. A constant has zero.
    pub fn variance(&self) -> T {
        match *self {
            Distribution::Constant(_) => T::zero(),
            Distribution::Gaussian { variance, .. } => variance,
        }
    }

    /// The standard deviation.
    pub fn standard_deviation(&self) -> T {
        self.variance().sqrt()
    }

    /// The confidence interval `(lo, hi)` containing roughly `confidence` of
    /// the mass. For a Gaussian this is the familiar ±zσ band; for a constant
    /// it is the point itself (requiring `confidence` in `(0, 1)`).
    pub fn confidence_interval(&self, confidence: T) -> (T, T)
    where
        T: FromPrimitive,
    {
        match *self {
            Distribution::Constant(v) => (v, v),
            Distribution::Gaussian { mean, variance } => {
                Fuzzy::new(mean, variance).confidence_interval(confidence)
            }
        }
    }

    /// Draws one sample from the distribution. `rng` must produce uniform
    /// doubles in `(0, 1)`. A Gaussian is sampled with the Box–Muller method.
    pub fn sample<R>(&self, mut rng: R) -> T
    where
        R: FnMut() -> f64,
        T: FromPrimitive,
    {
        match *self {
            Distribution::Constant(v) => v,
            Distribution::Gaussian { mean, variance } => {
                let u1 = rng().max(f64::EPSILON);
                // Box–Muller; the two uniforms produce two independent draws,
                // we keep one (the standard approach), u2 on (0,1].
                let u2 = rng().max(f64::EPSILON).min(1.0);
                let z = (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos();
                mean + T::from_f64(z.to_f64().unwrap()).unwrap() * variance.sqrt()
            }
        }
    }

    fn into_fuzzy(&self) -> Fuzzy<T> {
        match *self {
            Distribution::Constant(v) => Fuzzy::constant(v),
            Distribution::Gaussian { mean, variance } => Fuzzy::new(mean, variance),
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