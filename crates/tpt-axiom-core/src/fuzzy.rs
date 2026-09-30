//! The [`Fuzzy<T>`] probabilistic scalar type with automatic error propagation.

use core::fmt;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use num_traits::{Float, FromPrimitive};

/// A value whose true magnitude is uncertain, represented by a mean and a
/// variance (Gaussian / first-order error model).
///
/// Arithmetic on [`Fuzzy<T>`] propagates the uncertainty automatically using
/// the standard closed-form error-propagation rules. **The operands are
/// assumed statistically independent** — there is no covariance tracking —
/// so correlated inputs are silently treated as independent:
///
/// | Operation        | Mean                     | Variance                        |
/// |------------------|--------------------------|---------------------------------|
/// | `a + b` / `a - b`| `m_a ± m_b`              | `v_a + v_b`                     |
/// | `a * b`          | `m_a * m_b`              | `v_a v_b + v_a m_b² + v_b m_a²` |
/// | `a / b`          | `m_a / m_b`              | `(v_a m_b² + v_b m_a²) / m_b⁴`  |
/// | `a * c`, `c * a` | `c * m_a`                | `c² v_a`                        |
/// | `a / c`          | `m_a / c`                | `v_a / c²`                      |
/// | `c / a`          | `c / m_a`                | `c² v_a / m_a⁴`                 |
///
/// The `+`/`-`/`*` rows are exact under the independence + Gaussian model;
/// the `a / b` row is the **delta-method (first-order) approximation**, not
/// an identity. Two consequences worth internalizing:
///
/// * `x - x` has variance `2v` (twice the input's), not zero — the model has
///   no way to know the two operands are the *same* uncertain quantity.
/// * Degenerate arithmetic is IEEE-faithful rather than panicking: dividing
///   by a zero-mean value yields an infinite (or NaN) variance through the
///   internal unchecked constructor; use [`Fuzzy::checked_div`] or
///   [`Fuzzy::checked_fuse`] when a validated result is required.
///
/// Not `Eq`: float-backed (NaN means are reachable through the degenerate
/// arithmetic paths), so `Eq`'s reflexivity would be unsound.
#[allow(clippy::derive_partial_eq_without_eq)] // deliberate: float backing
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Fuzzy<T> {
    mean: T,
    variance: T,
}

impl<T: Float> Fuzzy<T> {
    /// Creates a value with the given `mean` and `variance`.
    ///
    /// # Panics
    ///
    /// Panics if `variance` is negative or NaN. A variance of zero makes the
    /// value deterministic and is always allowed.
    #[inline]
    pub fn new(mean: T, variance: T) -> Self {
        assert!(
            variance >= T::zero() && !variance.is_nan(),
            "variance must be non-negative and finite"
        );
        Self { mean, variance }
    }

    /// The internal constructor used by the operator impls: no validation,
    /// so IEEE degenerate results (infinite/NaN variance from division by a
    /// zero-mean estimate) surface as values instead of panics.
    #[inline]
    #[allow(clippy::missing_const_for_fn)] // plain fn: no benefit to const here
    fn new_raw(mean: T, variance: T) -> Self {
        Self { mean, variance }
    }

    /// Like the `/` operator, but rejects the degenerate cases the
    /// propagation formulas cannot express: a zero or non-finite divisor
    /// mean, or a non-finite resulting variance.
    ///
    /// # Errors
    /// Returns `rhs.mean()` (the rejected divisor) when the division is
    /// degenerate.
    pub fn checked_div(&self, rhs: &Self) -> Result<Self, T> {
        let mean = self.mean / rhs.mean;
        let b2 = rhs.mean * rhs.mean;
        let b4 = b2 * b2;
        let variance = (self.variance * b2 + rhs.variance * self.mean * self.mean) / b4;
        if rhs.mean.is_zero() || !rhs.mean.is_finite() || !variance.is_finite() {
            Err(rhs.mean)
        } else {
            Ok(Self::new_raw(mean, variance))
        }
    }

    /// A deterministic (certain) value, i.e. a `Fuzzy` with zero variance.
    #[inline]
    pub fn constant(value: T) -> Self {
        Self::new(value, T::zero())
    }

    /// Creates a value from a mean and a standard deviation.
    ///
    /// `Fuzzy` stores *variance* (the squared standard error); this
    /// constructor takes the more human-readable sigma form and squares it.
    /// `Fuzzy::new(m, v).standard_deviation() == sigma` up to rounding.
    #[inline]
    pub fn from_std_dev(mean: T, std_dev: T) -> Self {
        Self::new(mean, std_dev * std_dev)
    }

    /// The point estimate of the value.
    #[inline]
    pub const fn mean(&self) -> T {
        self.mean
    }

    /// The squared standard error: a measure of how uncertain the value is.
    #[inline]
    pub const fn variance(&self) -> T {
        self.variance
    }

    /// The standard deviation (`sqrt(variance)`).
    #[inline]
    pub fn standard_deviation(&self) -> T {
        self.variance.sqrt()
    }

    /// The one-sigma confidence interval `(mean - sigma, mean + sigma)`.
    #[inline]
    pub fn one_sigma(&self) -> (T, T) {
        let s = self.standard_deviation();
        (self.mean - s, self.mean + s)
    }

    /// The confidence interval containing approximately `confidence` of the
    /// probability mass (a Gaussian is assumed). `confidence` must be in the
    /// open interval `(0, 1)`; e.g. `0.95` gives a ±1.96σ interval.
    ///
    /// # Panics
    ///
    /// Panics if `confidence <= 0` or `confidence >= 1`.
    #[inline]
    pub fn confidence_interval(&self, confidence: T) -> (T, T)
    where
        T: FromPrimitive,
    {
        assert!(
            confidence > T::zero() && confidence < T::one(),
            "confidence level must lie in (0, 1)"
        );
        // `0.5 + c/2` never rounds up to exactly 1.0 until the last few
        // ulps below one (unlike `(1 + c)/2`, which does from c >= 0.9);
        // the clamp is the belt-and-braces for those last ulps.
        let tail = T::from_f64(0.5).unwrap() + confidence / (T::one() + T::one());
        let tail = tail.to_f64().unwrap().min(1.0 - f64::EPSILON);
        let z = T::from_f64(crate::quants::norm_ppf(tail)).unwrap();
        let half = z * self.standard_deviation();
        (self.mean - half, self.mean + half)
    }

    /// The z-score of a candidate `value`: how many standard deviations it
    /// lies above the mean.
    ///
    /// A zero-variance value has no spread to score against: the result is
    /// ±infinity (or NaN for `value == mean`, via IEEE `0/0`), never a
    /// meaningful distance.
    #[inline]
    pub fn z_score(&self, value: T) -> T {
        (value - self.mean) / self.standard_deviation()
    }

    /// Fuses two independent estimates of the same quantity into a single,
    /// tighter estimate (the minimum-variance / Kalman-style combination).
    ///
    /// ```text
    /// v = (v_a * v_b) / (v_a + v_b)
    /// m = (m_a * v_b + m_b * v_a) / (v_a + v_b)
    /// ```
    ///
    /// Degenerate inputs (both variances zero) follow IEEE semantics through
    /// the unchecked constructor (`0/0` variance); use [`Self::checked_fuse`]
    /// to reject them.
    #[inline]
    #[must_use]
    pub fn fuse(&self, other: &Self) -> Self {
        let v_sum = self.variance + other.variance;
        let variance = self.variance * other.variance / v_sum;
        let mean = (self.mean * other.variance + other.mean * self.variance) / v_sum;
        Self::new_raw(mean, variance)
    }

    /// Like [`Self::fuse`], but rejects inputs that make the combination
    /// degenerate: both variances zero (nothing to fuse), a non-finite
    /// variance sum, or a non-finite result.
    ///
    /// # Errors
    /// Returns `Err` with no payload when the fusion is degenerate.
    pub fn checked_fuse(&self, other: &Self) -> Result<Self, FuseError> {
        let v_sum = self.variance + other.variance;
        if (self.variance.is_zero() && other.variance.is_zero())
            || !v_sum.is_finite()
        {
            return Err(FuseError);
        }
        let variance = self.variance * other.variance / v_sum;
        let mean = (self.mean * other.variance + other.mean * self.variance) / v_sum;
        if !variance.is_finite() || !mean.is_finite() {
            return Err(FuseError);
        }
        Ok(Self::new_raw(mean, variance))
    }
}

/// A degenerate fusion: both inputs were certain (zero variance), or the
/// combination did not stay finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuseError;

impl fmt::Display for FuseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "cannot fuse: both variances were zero or the combination was not finite",
        )
    }
}

impl core::error::Error for FuseError {}

impl<T: Float> From<T> for Fuzzy<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::constant(value)
    }
}

impl<T: Float + fmt::Debug> fmt::Debug for Fuzzy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fuzzy(μ={:?}, σ²={:?})", self.mean, self.variance)
    }
}

impl<T: Float + fmt::Display> fmt::Display for Fuzzy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.*} ± {:.*}",
            f.precision().unwrap_or(4),
            self.mean,
            f.precision().unwrap_or(4),
            self.standard_deviation()
        )
    }
}

/// `(a + b).mean = a.mean + b.mean`, `(a + b).variance = a.variance + b.variance`.
impl<T: Float> Add for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new_raw(self.mean + rhs.mean, self.variance + rhs.variance)
    }
}

impl<T: Float> AddAssign for Fuzzy<T> {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// Adding a certain scalar: variance is unchanged.
impl<T: Float> Add<T> for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn add(self, rhs: T) -> Self::Output {
        Self::new_raw(self.mean + rhs, self.variance)
    }
}

impl<T: Float> AddAssign<T> for Fuzzy<T> {
    #[inline]
    fn add_assign(&mut self, rhs: T) {
        *self = *self + rhs;
    }
}

impl<T: Float> Add<T> for &Fuzzy<T> {
    type Output = Fuzzy<T>;
    #[inline]
    fn add(self, rhs: T) -> Self::Output {
        Fuzzy::new_raw(self.mean + rhs, self.variance)
    }
}

// `scalar + Fuzzy` / `scalar - Fuzzy` / etc. cannot be implemented generically
// over `T` because of the orphan rules (the bare type parameter `T` would appear
// before the local `Fuzzy<T>`), so they are generated for the float primitives.

/// `(a - b).mean = a.mean - b.mean`, variance still adds.
///
/// Note this models *independent* errors: `x - x` has variance `2v`, not
/// zero, because the model cannot see that both operands are the same
/// quantity.
impl<T: Float> Sub for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new_raw(self.mean - rhs.mean, self.variance + rhs.variance)
    }
}

impl<T: Float> SubAssign for Fuzzy<T> {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<T: Float> Sub<T> for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: T) -> Self::Output {
        Self::new_raw(self.mean - rhs, self.variance)
    }
}

impl<T: Float> SubAssign<T> for Fuzzy<T> {
    #[inline]
    fn sub_assign(&mut self, rhs: T) {
        *self = *self - rhs;
    }
}

impl<T: Float> Neg for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Self::new_raw(-self.mean, self.variance)
    }
}

impl<T: Float> Neg for &Fuzzy<T> {
    type Output = Fuzzy<T>;
    #[inline]
    fn neg(self) -> Self::Output {
        Fuzzy::new_raw(-self.mean, self.variance)
    }
}

/// Product mean is the product of means; variance follows the independence
/// rule `v_ab = v_a v_b + v_a m_b² + v_b m_a²`.
impl<T: Float> Mul for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        let mean = self.mean * rhs.mean;
        let variance = self.variance * rhs.variance
            + self.variance * rhs.mean * rhs.mean
            + rhs.variance * self.mean * self.mean;
        Self::new_raw(mean, variance)
    }
}

impl<T: Float> MulAssign for Fuzzy<T> {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

/// Scaling by a certain factor: variance scales with its square.
impl<T: Float> Mul<T> for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: T) -> Self::Output {
        Self::new_raw(self.mean * rhs, self.variance * rhs * rhs)
    }
}

impl<T: Float> MulAssign<T> for Fuzzy<T> {
    #[inline]
    fn mul_assign(&mut self, rhs: T) {
        *self = *self * rhs;
    }
}

impl<T: Float> Mul<T> for &Fuzzy<T> {
    type Output = Fuzzy<T>;
    #[inline]
    fn mul(self, rhs: T) -> Self::Output {
        Fuzzy::new_raw(self.mean * rhs, self.variance * rhs * rhs)
    }
}

/// Division mean is the quotient of means (requires `rhs.mean != 0`); the
/// variance rule is the **delta-method approximation** for independent
/// operands — an approximation, not an identity. Degenerate divisors produce
/// IEEE results (infinite/NaN variance) instead of panicking; see
/// [`Fuzzy::checked_div`] for the validated form.
impl<T: Float> Div for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        let mean = self.mean / rhs.mean;
        let b2 = rhs.mean * rhs.mean;
        let b4 = b2 * b2;
        let variance = (self.variance * b2 + rhs.variance * self.mean * self.mean) / b4;
        Self::new_raw(mean, variance)
    }
}

impl<T: Float> DivAssign for Fuzzy<T> {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl<T: Float> Div<T> for Fuzzy<T> {
    type Output = Self;
    #[inline]
    fn div(self, rhs: T) -> Self::Output {
        Self::new_raw(self.mean / rhs, self.variance / (rhs * rhs))
    }
}

impl<T: Float> DivAssign<T> for Fuzzy<T> {
    #[inline]
    fn div_assign(&mut self, rhs: T) {
        *self = *self / rhs;
    }
}

impl<T: Float> Div<T> for &Fuzzy<T> {
    type Output = Fuzzy<T>;
    #[inline]
    fn div(self, rhs: T) -> Self::Output {
        Fuzzy::new_raw(self.mean / rhs, self.variance / (rhs * rhs))
    }
}

/// `scalar + Fuzzy`, `scalar - Fuzzy`, `scalar * Fuzzy` and `scalar / Fuzzy`.
/// These cannot be written for a blanket `T` (the orphan rules reject a bare
/// type parameter in the `Self` position), so they are generated for `f32` and
/// `f64` — the floating-point types this crate targets.
macro_rules! impl_scalar_ops {
    ($($t:ty),* $(,)?) => {
        $(
            impl Add<Fuzzy<$t>> for $t {
                type Output = Fuzzy<$t>;
                #[inline]
                fn add(self, rhs: Fuzzy<$t>) -> Self::Output {
                    rhs + self
                }
            }

            impl Sub<Fuzzy<$t>> for $t {
                type Output = Fuzzy<$t>;
                #[inline]
                fn sub(self, rhs: Fuzzy<$t>) -> Self::Output {
                    Fuzzy::new_raw(self - rhs.mean(), rhs.variance())
                }
            }

            impl Mul<Fuzzy<$t>> for $t {
                type Output = Fuzzy<$t>;
                #[inline]
                fn mul(self, rhs: Fuzzy<$t>) -> Self::Output {
                    rhs * self
                }
            }

            impl Div<Fuzzy<$t>> for $t {
                type Output = Fuzzy<$t>;
                #[inline]
                fn div(self, rhs: Fuzzy<$t>) -> Self::Output {
                    let mean = self / rhs.mean();
                    let m2 = rhs.mean() * rhs.mean();
                    let m4 = m2 * m2;
                    Fuzzy::new_raw(mean, self * self * rhs.variance() / m4)
                }
            }
        )*
    };
}

impl_scalar_ops!(f32, f64);

#[cfg(feature = "serde")]
mod serde_impls {
    //! Deserialization re-validates the variance invariant: a serde payload
    //! is untrusted input, so a negative or NaN variance can never re-enter
    //! the type system through a deserializer.

    use super::{Float, Fuzzy};
    use serde::de::Error as DeError;
    use serde::{Deserialize, Deserializer};

    impl<'de, T: Deserialize<'de> + Float> Deserialize<'de> for Fuzzy<T> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Shadow<T> {
                mean: T,
                variance: T,
            }
            let shadow = Shadow::<T>::deserialize(deserializer)?;
            if shadow.variance.is_nan() || shadow.variance < T::zero() {
                Err(DeError::custom(
                    "variance must be non-negative and finite",
                ))
            } else {
                Ok(Self {
                    mean: shadow.mean,
                    variance: shadow.variance,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on analytic results
    #![allow(clippy::cast_precision_loss)] // deterministic seeded-RNG casts

    use super::*;
    use crate::quants::norm_cdf;

    #[test]
    fn accessors() {
        let x = Fuzzy::<f64>::new(3.0, 4.0);
        assert_eq!(x.mean(), 3.0);
        assert_eq!(x.variance(), 4.0);
        assert_eq!(x.standard_deviation(), 2.0);
        assert!((x.z_score(7.0) - 2.0).abs() < 1e-12);
    }

    #[test]
    #[should_panic(expected = "variance must be non-negative and finite")]
    fn rejects_negative_variance() {
        let _ = Fuzzy::<f64>::new(1.0, -1.0);
    }

    #[test]
    #[should_panic(expected = "variance must be non-negative and finite")]
    fn rejects_nan_variance() {
        let _ = Fuzzy::<f64>::new(1.0, f64::NAN);
    }

    #[test]
    fn add_variance_is_sum() {
        let a = Fuzzy::new(10.5_f64, 0.5);
        let b = Fuzzy::new(2.0_f64, 0.1);
        let c = a + b;
        assert!((c.mean() - 12.5).abs() < 1e-12);
        assert!((c.variance() - 0.6).abs() < 1e-12);
    }

    #[test]
    fn sub_variance_is_sum() {
        let a = Fuzzy::new(10.5_f64, 0.5);
        let b = Fuzzy::new(2.0_f64, 0.1);
        let c = a - b;
        assert!((c.mean() - 8.5).abs() < 1e-12);
        assert!((c.variance() - 0.6).abs() < 1e-12);
    }

    #[test]
    fn mul_propagation() {
        // Independent Gaussians. E[XY] = m_a m_b.
        let a = Fuzzy::new(3.0_f64, 0.25);
        let b = Fuzzy::new(2.0_f64, 0.5);
        let c = a * b;
        assert!((c.mean() - 6.0).abs() < 1e-12);
        // v = v_a v_b + v_a m_b^2 + v_b m_a^2
        let expected = 0.25 * 0.5 + 0.25 * 4.0 + 0.5 * 9.0;
        assert!((c.variance() - expected).abs() < 1e-12);
    }

    #[test]
    fn div_propagation() {
        let a = Fuzzy::new(8.0_f64, 1.0);
        let b = Fuzzy::new(2.0_f64, 0.25);
        let c = a / b;
        assert!((c.mean() - 4.0).abs() < 1e-12);
        // v = (v_a m_b^2 + v_b m_a^2) / m_b^4 = (1*4 + 0.25*64)/16 = (4+16)/16 = 1.25
        assert!((c.variance() - 1.25).abs() < 1e-12);
    }

    #[test]
    fn scalar_mixed_operators() {
        let a = Fuzzy::new(3.0_f64, 0.25);
        // a + c
        let c = a + 2.0;
        assert!((c.mean() - 5.0).abs() < 1e-12 && (c.variance() - 0.25).abs() < 1e-12);
        let c = 2.0 + a;
        assert!((c.mean() - 5.0).abs() < 1e-12 && (c.variance() - 0.25).abs() < 1e-12);
        // a * c
        let c = a * 2.0;
        assert!((c.mean() - 6.0).abs() < 1e-12 && (c.variance() - 1.0).abs() < 1e-12);
        let c = 2.0 * a;
        assert!((c.mean() - 6.0).abs() < 1e-12 && (c.variance() - 1.0).abs() < 1e-12);
        // a / c
        let c = a / 2.0;
        assert!((c.mean() - 1.5).abs() < 1e-12 && (c.variance() - 0.0625).abs() < 1e-12);
        // c / a
        let c = 8.0 / a;
        assert!((c.mean() - 8.0 / 3.0).abs() < 1e-12);
        assert!((c.variance() - 64.0 * 0.25 / 81.0).abs() < 1e-12);
        // c - a
        let c = 5.0 - a;
        assert!((c.mean() - 2.0).abs() < 1e-12 && (c.variance() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn confidence_interval_sigma_coverage() {
        let x = Fuzzy::new(10.0_f64, 4.0); // sigma = 2
        let (lo, hi) = x.confidence_interval(0.95);
        let z = 1.959_963_984_540_054;
        assert!((lo - (10.0 - z * 2.0)).abs() < 1e-6);
        assert!((hi - (10.0 + z * 2.0)).abs() < 1e-6);
        // 0.682_689_492_137_086 is the exact two-sided coverage of +/-1 sigma.
        let (lo, hi) = x.confidence_interval(0.682_689_492_137_086);
        assert!((lo - 8.0).abs() < 1e-6);
        assert!((hi - 12.0).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "confidence level must lie in (0, 1)")]
    fn confidence_interval_rejects_out_of_range() {
        let x = Fuzzy::new(10.0_f64, 4.0);
        let _ = x.confidence_interval(1.1);
    }

    #[test]
    fn fuse_tightens_estimate() {
        let a = Fuzzy::new(10.0_f64, 1.0);
        let b = Fuzzy::new(12.0_f64, 3.0);
        let fused = a.fuse(&b);
        // v = 1*3/(1+3) = 0.75; m = (10*3 + 12*1)/4 = 42/4 = 10.5
        assert!((fused.variance() - 0.75).abs() < 1e-12);
        assert!((fused.mean() - 10.5).abs() < 1e-12);
        // A fused estimate must be no more uncertain than either input.
        assert!(fused.variance() < a.variance());
        assert!(fused.variance() < b.variance());
    }

    #[test]
    fn constant_scalar_from() {
        let x: Fuzzy<f64> = 5.0.into();
        assert_eq!(x.mean(), 5.0);
        assert_eq!(x.variance(), 0.0);
    }

    #[test]
    fn assignment_operators() {
        let mut a = Fuzzy::new(1.0_f64, 0.1);
        a += 2.0;
        assert!((a.mean() - 3.0).abs() < 1e-12);
        a *= 3.0;
        assert!((a.mean() - 9.0).abs() < 1e-12);
        a -= 1.0;
        assert!((a.mean() - 8.0).abs() < 1e-12);
        a /= 2.0;
        assert!((a.mean() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn reference_operators_for_chains() {
        let vel = Fuzzy::new(2.0_f64, 0.1);
        let dt = 0.2_f64;
        // `&Fuzzy * T` via the reference implementations.
        let next = vel * dt;
        assert!((next.mean() - 0.4).abs() < 1e-12);
        assert!((next.variance() - 0.1 * 0.04).abs() < 1e-12);
        let summed = &vel + 1.0;
        assert!((summed.mean() - 3.0).abs() < 1e-12);
        let halved = &vel / 2.0;
        assert!((halved.mean() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn z_score_matches_cdf() {
        let x = Fuzzy::new(50.0_f64, 25.0); // sigma = 5
        assert!((x.z_score(55.0) - 1.0).abs() < 1e-12);
        // P(value in (40, 60)) should be ~1 - 2*Phi(-2) under the Gaussian model.
        let p_in = norm_cdf(2.0) - norm_cdf(-2.0);
        assert!((p_in - 0.9545).abs() < 1e-3);
    }

    #[test]
    fn degenerate_division_does_not_panic() {
        // Dividing by a zero-mean estimate: IEEE semantics through the
        // unchecked internal constructor, not a panic.
        let a = Fuzzy::new(1.0_f64, 1.0);
        let zero_mean = Fuzzy::new(0.0_f64, 1.0);
        let q = a / zero_mean;
        assert!(!q.variance().is_finite(), "0/0 variance is IEEE, not a panic");

        // The checked form names the rejected divisor.
        assert_eq!(a.checked_div(&zero_mean), Err(0.0));
        // (v_a m_b^2 + v_b m_a^2)/m_b^4 = (1*4 + 1*1)/16 = 0.3125.
        assert_eq!(
            a.checked_div(&Fuzzy::new(2.0, 1.0)),
            Ok(Fuzzy::new(0.5, 0.3125))
        );
    }

    #[test]
    fn inf_variance_arithmetic_does_not_panic() {
        let inf_var = Fuzzy::new(1.0_f64, f64::INFINITY);
        let x = Fuzzy::new(1.0_f64, 0.0);
        // 0 * inf variance → NaN in the product rule; the operators produce
        // the IEEE value instead of panicking on a NaN variance.
        let product = x * inf_var;
        assert!(product.variance().is_nan());
        let sum = x + inf_var;
        assert!(sum.variance().is_infinite());
    }

    #[test]
    fn fuse_degenerate_variants() {
        let a = Fuzzy::new(10.0_f64, 1.0);
        let b = Fuzzy::new(12.0_f64, 3.0);
        assert!(a.checked_fuse(&b).is_ok());
        // Both variances zero: fuse follows IEEE (0/0); checked_fuse rejects.
        let c = Fuzzy::constant(1.0_f64);
        let _ = c.fuse(&c); // must not panic
        assert_eq!(c.checked_fuse(&c), Err(crate::FuseError));
    }

    #[test]
    fn confidence_interval_tolerates_extreme_confidence() {
        let x = Fuzzy::new(10.0_f64, 4.0);
        // confidence so close to 1 that (1 + c)/2 would round to exactly
        // 1.0 and panic inside norm_ppf; 0.5 + c/2 never does.
        let c = 1.0 - 1e-16;
        let (lo, hi) = x.confidence_interval(c);
        assert!(lo < 10.0 && hi > 10.0);
        // The half-width is the (clamped) ~8.09-sigma quantile.
        let half = (hi - lo) / 2.0;
        assert!(half.is_finite() && half > 0.0);
    }
}
