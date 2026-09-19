//! Operator overloading for [`Fuzzy<T>`], implementing error-propagation
//! arithmetic under the assumption that operands are independent.

use core::ops::{Add, Div, Mul, Sub};

use crate::{Fuzzy, traits::FuzzyScalar};

impl<T: FuzzyScalar> Add for Fuzzy<T> {
    type Output = Self;

    /// `mean = a.mean + b.mean`; independent variances add:
    /// `variance = a.variance + b.variance`.
    fn add(self, rhs: Self) -> Self {
        Self::new(self.mean() + rhs.mean(), self.variance() + rhs.variance())
    }
}

impl<T: FuzzyScalar> Sub for Fuzzy<T> {
    type Output = Self;

    /// `mean = a.mean - b.mean`; independent variances still add:
    /// `variance = a.variance + b.variance`.
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.mean() - rhs.mean(), self.variance() + rhs.variance())
    }
}

impl<T: FuzzyScalar> Mul for Fuzzy<T> {
    type Output = Self;

    /// Exact variance of a product of independent random variables:
    /// `Var(XY) = E[X]^2 Var(Y) + E[Y]^2 Var(X) + Var(X) Var(Y)`.
    fn mul(self, rhs: Self) -> Self {
        let mean = self.mean() * rhs.mean();
        let variance = self.mean() * self.mean() * rhs.variance()
            + rhs.mean() * rhs.mean() * self.variance()
            + self.variance() * rhs.variance();
        Self::new(mean, variance)
    }
}

impl<T: FuzzyScalar> Div for Fuzzy<T> {
    type Output = Self;

    /// First-order (delta-method) approximation of the variance of a ratio of
    /// independent random variables:
    /// `Var(X/Y) ≈ Var(X)/E[Y]^2 + E[X]^2 Var(Y)/E[Y]^4`.
    fn div(self, rhs: Self) -> Self {
        let mean = self.mean() / rhs.mean();
        let rhs_mean_sq = rhs.mean() * rhs.mean();
        let variance = self.variance() / rhs_mean_sq
            + self.mean() * self.mean() * rhs.variance() / (rhs_mean_sq * rhs_mean_sq);
        Self::new(mean, variance)
    }
}

impl<T: FuzzyScalar> Add<T> for Fuzzy<T> {
    type Output = Self;

    /// Adding a constant shifts the mean; a constant has no variance, so the
    /// variance is unchanged.
    fn add(self, rhs: T) -> Self {
        Self::new(self.mean() + rhs, self.variance())
    }
}

impl<T: FuzzyScalar> Sub<T> for Fuzzy<T> {
    type Output = Self;

    /// Subtracting a constant shifts the mean; the variance is unchanged.
    fn sub(self, rhs: T) -> Self {
        Self::new(self.mean() - rhs, self.variance())
    }
}

impl<T: FuzzyScalar> Mul<T> for Fuzzy<T> {
    type Output = Self;

    /// Scaling by a constant `c` scales the mean by `c` and the variance by
    /// `c^2` (`Var(cX) = c^2 Var(X)`).
    fn mul(self, rhs: T) -> Self {
        Self::new(self.mean() * rhs, self.variance() * rhs * rhs)
    }
}

impl<T: FuzzyScalar> Div<T> for Fuzzy<T> {
    type Output = Self;

    /// Dividing by a constant `c` divides the mean by `c` and the variance by
    /// `c^2`.
    fn div(self, rhs: T) -> Self {
        Self::new(self.mean() / rhs, self.variance() / (rhs * rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-9;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() <= EPSILON
    }

    #[test]
    fn add_propagates_variance_additively() {
        let sum = Fuzzy::new(2.0, 0.5) + Fuzzy::new(3.0, 0.25);
        assert!(approx_eq(sum.mean(), 5.0));
        assert!(approx_eq(sum.variance(), 0.75));
    }

    #[test]
    fn sub_propagates_variance_additively() {
        let diff = Fuzzy::new(5.0, 0.5) - Fuzzy::new(3.0, 0.25);
        assert!(approx_eq(diff.mean(), 2.0));
        assert!(approx_eq(diff.variance(), 0.75));
    }

    #[test]
    fn mul_matches_closed_form_product_variance() {
        let a = Fuzzy::new(2.0, 0.5);
        let b = Fuzzy::new(3.0, 0.25);
        let product = a * b;
        assert!(approx_eq(product.mean(), 6.0));
        let expected_variance =
            0.5f64.mul_add(0.25, 3.0f64.powi(2).mul_add(0.5, 2.0f64.powi(2) * 0.25));
        assert!(approx_eq(product.variance(), expected_variance));
    }

    #[test]
    fn div_matches_delta_method_variance() {
        let a = Fuzzy::new(6.0, 0.5);
        let b = Fuzzy::new(3.0, 0.25);
        let ratio = a / b;
        assert!(approx_eq(ratio.mean(), 2.0));
        let expected_variance = 0.5 / (3.0 * 3.0) + (6.0 * 6.0 * 0.25) / (3.0f64.powi(4));
        assert!(approx_eq(ratio.variance(), expected_variance));
    }

    #[test]
    fn scalar_add_leaves_variance_unchanged() {
        let a = Fuzzy::new(2.0, 0.5) + 10.0;
        assert!(approx_eq(a.mean(), 12.0));
        assert!(approx_eq(a.variance(), 0.5));
    }

    #[test]
    fn scalar_sub_leaves_variance_unchanged() {
        let a = Fuzzy::new(12.0, 0.5) - 10.0;
        assert!(approx_eq(a.mean(), 2.0));
        assert!(approx_eq(a.variance(), 0.5));
    }

    #[test]
    fn scalar_mul_scales_variance_by_square() {
        let a = Fuzzy::new(2.0, 0.5) * 3.0;
        assert!(approx_eq(a.mean(), 6.0));
        assert!(approx_eq(a.variance(), 0.5 * 9.0));
    }

    #[test]
    fn scalar_div_scales_variance_by_inverse_square() {
        let a = Fuzzy::new(6.0, 4.5) / 3.0;
        assert!(approx_eq(a.mean(), 2.0));
        assert!(approx_eq(a.variance(), 4.5 / 9.0));
    }
}
