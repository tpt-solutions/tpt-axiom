//! Correlated uncertainty: a mean vector with a full covariance matrix, and
//! propagation under linear maps — where the independence assumption behind
//! [`Fuzzy`](crate::Fuzzy) arithmetic is *not* made.
//!
//! `Fuzzy` operators treat every operand as independent, so `x − x` has
//! variance `2v` even when both operands are the *same* uncertain quantity.
//! [`Correlated`] carries the covariances explicitly, which makes the
//! operations that matter exact rather than approximations:
//!
//! * a weighted sum `Σ wᵢxᵢ` propagates as `wᵀΣw` — positively correlated
//!   errors compound faster than independent ones, negatively correlated
//!   errors cancel;
//! * a linear map `y = Ax` propagates as `AΣAᵀ`;
//! * with correlation +1 and equal variances, `x − x` is exactly certain.
//!
//! Nonlinear maps stay out of scope: they need Jacobians per transform and
//! are tracked in `todo.md` (Phase D). Everything here is exact linear
//! algebra under the Gaussian model.

// Without std, the f64 float methods come from num-traits (libm).
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
#[allow(unused_imports)]
use num_traits::Float as _;

use crate::Fuzzy;

/// Why a [`Correlated`] construction or linear map was rejected.
#[derive(Clone, Debug, PartialEq)]
pub enum CorrelationError {
    /// The weight vector or matrix dimension disagreed with the state.
    DimensionMismatch {
        /// The dimension the state has.
        expected: usize,
        /// The dimension supplied.
        got: usize,
    },
    /// A mean, variance, covariance entry, weight, or matrix row was not
    /// finite.
    NotFinite,
    /// A diagonal (variance) entry was negative.
    NegativeVariance(f64),
    /// The covariance matrix was not symmetric (within `1e-9` relative).
    NotSymmetric,
    /// The covariance matrix is not positive semi-definite (a Cholesky
    /// pivot went non-positive), e.g. a correlation outside `[-1, 1]`.
    NotPositiveSemidefinite {
        /// The index of the failing pivot.
        pivot: usize,
    },
}

impl fmt::Display for CorrelationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionMismatch { expected, got } => {
                write!(f, "expected dimension {expected}, got {got}")
            }
            Self::NotFinite => f.write_str("entries must be finite"),
            Self::NegativeVariance(v) => write!(f, "variance must be non-negative, got {v}"),
            Self::NotSymmetric => f.write_str("covariance matrix must be symmetric"),
            Self::NotPositiveSemidefinite { pivot } => write!(
                f,
                "covariance matrix is not positive semi-definite (pivot {pivot} ≤ 0)"
            ),
        }
    }
}

impl core::error::Error for CorrelationError {}

/// A mean vector with a full covariance matrix: the correlated
/// counterpart of a slice of [`Fuzzy`] estimates.
///
/// Construct with [`Correlated::independent`] (diagonal), [`Correlated::pair`]
/// (two estimates plus a correlation), or [`Correlated::new`] with an
/// explicit matrix (validated: finite, symmetric, positive semi-definite via
/// Cholesky). Propagate through [`Correlated::weighted_sum`] and
/// [`Correlated::linear_map`].
#[derive(Clone, Debug, PartialEq)]
pub struct Correlated {
    means: Vec<f64>,
    cov: Vec<Vec<f64>>,
}

impl Correlated {
    /// Builds from an explicit mean vector and covariance matrix.
    ///
    /// The matrix is validated for finiteness, symmetry (within `1e-9`
    /// relative), non-negative variances, and positive semi-definiteness
    /// (Cholesky) — a matrix that is not PSD is not a covariance matrix,
    /// and accepting it would let negative variances flow downstream.
    ///
    /// # Errors
    /// [`CorrelationError`] naming the first violated property.
    pub fn new(means: Vec<f64>, cov: Vec<Vec<f64>>) -> Result<Self, CorrelationError> {
        let n = means.len();
        if cov.len() != n || cov.iter().any(|row| row.len() != n) {
            return Err(CorrelationError::DimensionMismatch {
                expected: n,
                got: cov.len(),
            });
        }
        if means.iter().any(|m| !m.is_finite()) {
            return Err(CorrelationError::NotFinite);
        }
        for (i, row) in cov.iter().enumerate() {
            for (j, &c) in row.iter().enumerate() {
                if !c.is_finite() {
                    return Err(CorrelationError::NotFinite);
                }
                if i == j && c < 0.0 {
                    return Err(CorrelationError::NegativeVariance(c));
                }
                // Symmetry within a relative tolerance; exact symmetry is
                // too strict for hand-assembled matrices.
                if j > i && (c - cov[j][i]).abs() > 1e-9 * (c.abs() + cov[j][i].abs() + 1.0) {
                    return Err(CorrelationError::NotSymmetric);
                }
            }
        }
        cholesky_pivot(&cov)
            .map_err(|pivot| CorrelationError::NotPositiveSemidefinite { pivot })?;
        Ok(Self { means, cov })
    }

    /// Uncorrelated estimates: a diagonal covariance matrix.
    ///
    /// # Errors
    /// [`CorrelationError`] on non-finite input or a negative variance.
    pub fn independent(means: &[f64], variances: &[f64]) -> Result<Self, CorrelationError> {
        if means.len() != variances.len() {
            return Err(CorrelationError::DimensionMismatch {
                expected: means.len(),
                got: variances.len(),
            });
        }
        let n = means.len();
        let mut cov = vec![vec![0.0; n]; n];
        for (i, &v) in variances.iter().enumerate() {
            cov[i][i] = v;
        }
        Self::new(means.to_vec(), cov)
    }

    /// Two estimates with correlation `corr ∈ (-1, 1)` (exclusive: ±1 makes
    /// the matrix singular, which only a redundancy — not a model — should
    /// assert).
    ///
    /// # Errors
    /// [`CorrelationError`] on non-finite input, a negative variance, or
    /// `|corr| ≥ 1`.
    pub fn pair(m1: f64, v1: f64, m2: f64, v2: f64, corr: f64) -> Result<Self, CorrelationError> {
        if !corr.is_finite() || !(-1.0..1.0).contains(&corr) {
            return Err(CorrelationError::NotPositiveSemidefinite { pivot: 0 });
        }
        Self::new(
            vec![m1, m2],
            vec![
                vec![v1, corr * (v1 * v2).sqrt()],
                vec![corr * (v1 * v2).sqrt(), v2],
            ],
        )
    }

    /// The number of estimates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.means.len()
    }

    /// `true` when there are no estimates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.means.is_empty()
    }

    /// The mean vector.
    #[must_use]
    pub fn means(&self) -> &[f64] {
        &self.means
    }

    /// The covariance matrix (row-major, symmetric).
    #[must_use]
    pub fn covariance(&self) -> &[Vec<f64>] {
        &self.cov
    }

    /// The correlation of estimates `i` and `j` (1.0 on the diagonal).
    ///
    /// # Panics
    /// Panics if `i` or `j` is out of range.
    #[must_use]
    pub fn correlation(&self, i: usize, j: usize) -> f64 {
        assert!(i < self.len() && j < self.len(), "index out of range");
        if i == j {
            return 1.0;
        }
        self.cov[i][j] / (self.cov[i][i] * self.cov[j][j]).sqrt()
    }

    /// The exact variance of the weighted sum `Σ wᵢxᵢ`: `wᵀΣw`.
    ///
    /// This is the operation the independence assumption gets wrong — with
    /// correlation +1 and equal variances, the weights `(1, −1)` yield a
    /// *zero*-variance difference (certain cancellation), while
    /// independent propagation would report `2v`.
    ///
    /// # Errors
    /// [`CorrelationError::DimensionMismatch`] when the weights do not cover
    /// every estimate; [`CorrelationError::NotFinite`] for non-finite
    /// weights.
    pub fn weighted_sum(&self, weights: &[f64]) -> Result<Fuzzy<f64>, CorrelationError> {
        if weights.len() != self.len() {
            return Err(CorrelationError::DimensionMismatch {
                expected: self.len(),
                got: weights.len(),
            });
        }
        if weights.iter().any(|w| !w.is_finite()) {
            return Err(CorrelationError::NotFinite);
        }
        let mean: f64 = self.means.iter().zip(weights).map(|(m, w)| m * w).sum();
        // wᵀΣw, computed as Σᵢ wᵢ (Σw)ᵢ.
        let var = self.cov.iter().zip(weights).fold(0.0, |acc, (row, &wi)| {
            acc + wi * row.iter().zip(weights).map(|(&c, &wj)| c * wj).sum::<f64>()
        });
        Ok(Fuzzy::new(mean, var))
    }

    /// The linear map `y = Ax` for a row-major `k × n` matrix `a`:
    /// means transform as `Am`, the covariance as `AΣAᵀ`.
    ///
    /// # Errors
    /// [`CorrelationError::DimensionMismatch`] when a row's length is not
    /// the state dimension; [`CorrelationError::NotFinite`] for non-finite
    /// entries.
    pub fn linear_map(&self, a: &[Vec<f64>]) -> Result<Self, CorrelationError> {
        let n = self.len();
        if a.iter().any(|row| row.len() != n) {
            return Err(CorrelationError::DimensionMismatch {
                expected: n,
                got: a.first().map_or(0, Vec::len),
            });
        }
        if a.iter().any(|row| row.iter().any(|x| !x.is_finite())) {
            return Err(CorrelationError::NotFinite);
        }
        // means' = A m
        let means: Vec<f64> = a
            .iter()
            .map(|row| row.iter().zip(&self.means).map(|(&ai, &mi)| ai * mi).sum())
            .collect();
        // cov' = A Σ Aᵀ, the plain triple product — quadratic in the state
        // dimension, which is the right shape for the small vectors this
        // type is for, and impossible to get subtly wrong.
        let mut cov = vec![vec![0.0; a.len()]; a.len()];
        for (i, row_i) in a.iter().enumerate() {
            for (j, row_j) in a.iter().enumerate().skip(i) {
                let mut entry = 0.0;
                for (k, &aik) in row_i.iter().enumerate() {
                    for (l, &ajl) in row_j.iter().enumerate() {
                        entry += aik * ajl * self.cov[k][l];
                    }
                }
                cov[i][j] = entry;
                cov[j][i] = entry;
            }
        }
        Self::new(means, cov)
    }
}

/// The smallest Cholesky pivot of `m`, or the index of the first
/// non-positive one — the PSD test behind [`Correlated::new`].
fn cholesky_pivot(m: &[Vec<f64>]) -> Result<f64, usize> {
    let n = m.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let sum = m[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            let pivot = if i == j { sum } else { sum / l[j][j] };
            l[i][j] = pivot;
        }
        // Relative tolerance: a matrix that is PSD up to rounding passes.
        // (`!(a > b)` rather than `a <= b`: the pivot can be NaN.)
        let scale = m[i][i].abs().max(1.0);
        if l[i][i] <= -1e-10 * scale || l[i][i].is_nan() {
            return Err(i);
        }
        l[i][i] = l[i][i].max(0.0);
    }
    Ok(l.iter()
        .map(|row| row[0])
        .fold(f64::INFINITY, f64::min)
        .max(0.0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact closed forms on hand-built inputs
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn independent_matches_fuzzy_arithmetic() {
        let c = Correlated::independent(&[10.0, 2.0], &[1.0, 0.25]).unwrap();
        let sum = c.weighted_sum(&[1.0, 1.0]).unwrap();
        // Same as Fuzzy::new(10, 1) + Fuzzy::new(2, 0.25).
        assert_eq!(sum.mean(), 12.0);
        assert_eq!(sum.variance(), 1.25);
        let diff = c.weighted_sum(&[1.0, -1.0]).unwrap();
        assert_eq!(diff.mean(), 8.0);
        assert_eq!(diff.variance(), 1.25, "independent errors still add");
    }

    #[test]
    fn perfect_correlation_makes_the_difference_certain() {
        // The case the independence assumption cannot express: x − x with
        // correlated errors is exactly known.
        let c = Correlated::pair(10.0, 4.0, 10.0, 4.0, 1.0 - 1e-9).unwrap();
        let diff = c.weighted_sum(&[1.0, -1.0]).unwrap();
        assert_eq!(diff.mean(), 0.0);
        assert!(
            diff.variance() < 1e-6,
            "correlated difference must collapse, got {}",
            diff.variance()
        );
    }

    #[test]
    fn correlation_scales_the_sum_variance() {
        // Var(X+Y) = v1 + v2 + 2ρ√(v1v2).
        let c = Correlated::pair(1.0, 4.0, 2.0, 9.0, 0.5).unwrap();
        let sum = c.weighted_sum(&[1.0, 1.0]).unwrap();
        assert!(close(sum.variance(), 4.0 + 9.0 + 2.0 * 0.5 * 6.0, 1e-12));
        assert!(close(c.correlation(0, 1), 0.5, 1e-12));
    }

    #[test]
    fn linear_map_propagates_the_whole_matrix() {
        let c = Correlated::pair(1.0, 4.0, 2.0, 9.0, 0.5).unwrap();
        // y = (x1 + x2, x2): first row of AΣAᵀ equals the weighted sum.
        let y = c.linear_map(&[vec![1.0, 1.0], vec![0.0, 1.0]]).unwrap();
        assert_eq!(y.means(), &[3.0, 2.0]);
        let sum = c.weighted_sum(&[1.0, 1.0]).unwrap();
        assert!(close(y.covariance()[0][0], sum.variance(), 1e-9));
        // Var(x2) is unchanged.
        assert!(close(y.covariance()[1][1], 9.0, 1e-12));
        // Cross term: Cov(x1+x2, x2) = Cov(x1,x2) + Var(x2).
        let expected_cross = 0.5 * 6.0 + 9.0;
        assert!(close(y.covariance()[0][1], expected_cross, 1e-9));
    }

    #[test]
    fn non_psd_matrix_is_rejected() {
        // Correlation 2 is impossible.
        let err =
            Correlated::new(vec![0.0, 0.0], vec![vec![1.0, 2.0], vec![2.0, 1.0]]).unwrap_err();
        assert_eq!(
            err,
            CorrelationError::NotPositiveSemidefinite { pivot: 1 },
            "{err}"
        );
        // ±1 correlation exactly is singular: use pair's exclusive range.
        assert!(Correlated::pair(0.0, 1.0, 0.0, 1.0, 1.0).is_err());
        assert!(Correlated::pair(0.0, 1.0, 0.0, 1.0, f64::NAN).is_err());
    }

    #[test]
    fn malformed_inputs_are_rejected() {
        assert_eq!(
            Correlated::independent(&[1.0], &[1.0, 2.0]).unwrap_err(),
            CorrelationError::DimensionMismatch {
                expected: 1,
                got: 2
            }
        );
        assert_eq!(
            Correlated::new(vec![1.0], vec![vec![1.0, 0.5]]).unwrap_err(),
            CorrelationError::DimensionMismatch {
                expected: 1,
                got: 1
            }
        );
        assert_eq!(
            Correlated::independent(&[1.0], &[-1.0]).unwrap_err(),
            CorrelationError::NegativeVariance(-1.0)
        );
        assert!(Correlated::independent(&[f64::NAN], &[1.0]).is_err());
        assert_eq!(
            Correlated::new(vec![0.0, 0.0], vec![vec![1.0, 0.5], vec![0.9, 1.0]],).unwrap_err(),
            CorrelationError::NotSymmetric
        );
        // Non-finite covariance entry.
        assert!(
            Correlated::new(
                vec![0.0, 0.0],
                vec![vec![1.0, f64::NAN], vec![f64::NAN, 1.0]],
            )
            .is_err()
        );
    }

    #[test]
    fn dimension_mismatch_on_maps() {
        let c = Correlated::pair(0.0, 1.0, 0.0, 1.0, 0.0).unwrap();
        assert!(c.weighted_sum(&[1.0]).is_err());
        assert!(c.linear_map(&[vec![1.0, 2.0, 3.0]]).is_err());
        assert!(c.linear_map(&[vec![1.0, f64::NAN]]).is_err());
        assert!(c.weighted_sum(&[1.0, f64::NAN]).is_err());
    }
}
