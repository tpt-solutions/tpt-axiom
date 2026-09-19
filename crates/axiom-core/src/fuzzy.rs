//! [`Fuzzy<T>`]: a scalar value paired with its variance.

use num_traits::Float;

use crate::traits::FuzzyScalar;

/// A scalar value paired with its variance, propagating uncertainty through
/// arithmetic via the operator overloads in [`crate::operator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fuzzy<T> {
    mean: T,
    variance: T,
}

impl<T: FuzzyScalar> Fuzzy<T> {
    /// Construct a new `Fuzzy<T>` from a mean and a variance.
    pub const fn new(mean: T, variance: T) -> Self {
        Self { mean, variance }
    }

    /// The expected value.
    pub const fn mean(&self) -> T {
        self.mean
    }

    /// The propagated variance.
    pub const fn variance(&self) -> T {
        self.variance
    }
}

impl<T: FuzzyScalar + Float> Fuzzy<T> {
    /// Standard deviation, `sqrt(variance)`.
    pub fn std_dev(&self) -> T {
        self.variance.sqrt()
    }

    /// A `z`-sigma confidence interval around the mean:
    /// `(mean - z * std_dev, mean + z * std_dev)`.
    ///
    /// For example `z = 1.959_964` gives the (approximate) 95% interval for a
    /// Gaussian.
    pub fn confidence_interval(&self, z: T) -> (T, T) {
        let spread = z * self.std_dev();
        (self.mean - spread, self.mean + spread)
    }

    /// The z-score of an observed value against this distribution:
    /// `(observed - mean) / std_dev`.
    pub fn z_score(&self, observed: T) -> T {
        (observed - self.mean) / self.std_dev()
    }
}
