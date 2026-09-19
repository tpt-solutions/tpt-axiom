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

impl<T: FuzzyScalar> From<Fuzzy<T>> for Distribution<T> {
    fn from(fuzzy: Fuzzy<T>) -> Self {
        Self::gaussian(fuzzy.mean(), fuzzy.variance())
    }
}

impl<T: FuzzyScalar> From<Distribution<T>> for Fuzzy<T> {
    fn from(distribution: Distribution<T>) -> Self {
        Self::new(distribution.mean(), distribution.variance())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_fuzzy() {
        let d = Distribution::gaussian(1.5, 0.25);
        let f: Fuzzy<f64> = d.into();
        let back: Distribution<f64> = f.into();
        assert_eq!(d, back);
    }
}
