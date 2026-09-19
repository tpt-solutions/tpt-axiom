//! Scalar traits shared across `axiom-core`.
//!
//! The probabilistic operator overloads in [`crate::operator`] and the
//! statistical helpers in [`crate::fuzzy`] are generic over every real-number
//! scalar `T`.  Rather than repeating a wall of `where T: Copy + Add + ...`
//! on every impl, we name the full requirement once as [`FuzzyScalar`].
//!
//! `FuzzyScalar` is deliberately sealed-ish in spirit (blanket-implemented for
//! any type meeting the bounds) so downstream crates never need to name it;
//! every `Fuzzy<f64>`/`Fuzzy<f32>`/`Fuzzy<u64>` just works.

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Sub};

/// A scalar that supports the four field operations and enough structure for
/// error propagation: `Copy`, full ordering, and `Debug` for diagnostics.
pub trait FuzzyScalar:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + PartialOrd
    + Debug
    + 'static
{
}

impl<T> FuzzyScalar for T where
    T: Copy
        + Add<Output = Self>
        + Sub<Output = Self>
        + Mul<Output = Self>
        + Div<Output = Self>
        + PartialOrd
        + Debug
        + 'static
{
}
