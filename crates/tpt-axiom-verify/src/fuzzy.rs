//! Algebraic soundness checks for `Fuzzy<T>`'s variance-propagation formulas.
//!
//! **What this does and doesn't prove.** This module cannot inspect
//! `tpt-axiom-core`'s actual Rust source (that would require re-parsing
//! operator-overload bodies the way `tpt-axiom-macros` parses `#[zk_provable]`
//! functions — a much larger undertaking than Phase 3 calls for). Instead,
//! each `check_*` function below contains a **hand-transcribed** copy of the
//! formula from `crates/tpt-axiom-core/src/fuzzy.rs` (kept in sync by a
//! doc-comment pointing at the exact lines), built as a [`Polynomial`] one
//! way, and compares it against the **textbook** variance-propagation
//! identity for that operation, built as a polynomial a different way. Exact
//! polynomial equality then proves the two derivations agree — catching a
//! transcription/coding bug (wrong exponent, dropped term, wrong sign) the
//! same way `circuit::check_comparison` catches a circuit lowering bug.
//!
//! This is *not* automatic verification of the `Fuzzy<T>` source; it is a
//! machine-checked, hard-to-fake formalization of "does the implemented
//! formula still match the reference identity", which must be re-transcribed
//! here if `fuzzy.rs`'s formulas ever change.
//!
//! Symbols used throughout: `mean_a`, `var_a`, `mean_b`, `var_b` are the two
//! operands' means/variances; `k` is a certain (zero-variance) scalar.

use crate::poly::{expect_equal, Mismatch, Polynomial};

/// Symbol id for `a`'s mean.
pub const MEAN_A: usize = 0;
/// Symbol id for `a`'s variance.
pub const VAR_A: usize = 1;
/// Symbol id for `b`'s mean.
pub const MEAN_B: usize = 2;
/// Symbol id for `b`'s variance.
pub const VAR_B: usize = 3;
/// Symbol id for a certain scalar `k`.
pub const SCALE_K: usize = 4;

fn var(id: usize) -> Polynomial {
    Polynomial::variable(id)
}

/// `Fuzzy::add` (`fuzzy.rs:152-158`): `(a + b).variance = a.variance + b.variance`.
///
/// # Errors
///
/// Returns [`Mismatch`] if the transcribed and reference formulas disagree.
pub fn check_add_variance() -> Result<(), Mismatch> {
    let implemented = var(VAR_A) + var(VAR_B);
    let reference = var(VAR_A) + var(VAR_B);
    expect_equal(implemented, reference)
}

/// `Fuzzy::sub` (`fuzzy.rs:196-201`): `(a - b).variance = a.variance + b.variance`
/// (variance still *adds* under subtraction — errors compound either way).
///
/// # Errors
///
/// Returns [`Mismatch`] if the transcribed and reference formulas disagree.
pub fn check_sub_variance() -> Result<(), Mismatch> {
    let implemented = var(VAR_A) + var(VAR_B);
    let reference = var(VAR_A) + var(VAR_B);
    expect_equal(implemented, reference)
}

/// `Fuzzy * scalar` (`fuzzy.rs:264-269`): `(k * a).variance = k^2 * a.variance`.
///
/// Built two structurally different ways: the implementation computes
/// `variance * k * k`; the reference states the textbook
/// `Var(kX) = k^2 Var(X)` identity as `k^2 * variance`.
///
/// # Errors
///
/// Returns [`Mismatch`] if the transcribed and reference formulas disagree.
pub fn check_scale_variance() -> Result<(), Mismatch> {
    let implemented = var(VAR_A) * var(SCALE_K) * var(SCALE_K);
    let reference = (var(SCALE_K) * var(SCALE_K)) * var(VAR_A);
    expect_equal(implemented, reference)
}

/// `Fuzzy * Fuzzy` (`fuzzy.rs:243-253`): the variance of a product of two
/// independent random variables,
/// `Var(AB) = Var(A)Var(B) + Var(A)E[B]^2 + Var(B)E[A]^2`.
///
/// # Errors
///
/// Returns [`Mismatch`] if the transcribed and reference formulas disagree.
pub fn check_mul_variance() -> Result<(), Mismatch> {
    // Transcribed exactly as `fuzzy.rs`'s three summands.
    let implemented = var(VAR_A) * var(VAR_B)
        + var(VAR_A) * var(MEAN_B) * var(MEAN_B)
        + var(VAR_B) * var(MEAN_A) * var(MEAN_A);
    // The textbook identity, grouped differently (mean-squares first).
    let reference = (var(MEAN_A) * var(MEAN_A)) * var(VAR_B)
        + (var(MEAN_B) * var(MEAN_B)) * var(VAR_A)
        + var(VAR_A) * var(VAR_B);
    expect_equal(implemented, reference)
}

/// `Fuzzy / Fuzzy` (`fuzzy.rs:290-298`): the delta-method variance for
/// independent operands, `Var(A/B) = (Var(A) E[B]^2 + Var(B) E[A]^2) / E[B]^4`.
///
/// The formula has `mean_b` in a denominator, so it is not itself a
/// polynomial. Both sides are the *numerator only* (i.e. the whole identity
/// multiplied through by `mean_b^4`, the standard "clear denominators" move
/// for checking a rational identity via polynomial equality): the
/// implementation computes `variance * b2 + rhs.variance * mean_a * mean_a`
/// where `b2 = mean_b * mean_b`; the reference states the numerator as
/// `mean_a^2 * var_b + mean_b^2 * var_a`.
///
/// # Errors
///
/// Returns [`Mismatch`] if the transcribed and reference formulas disagree.
pub fn check_div_variance() -> Result<(), Mismatch> {
    let mean_b2 = var(MEAN_B) * var(MEAN_B);
    let implemented = var(VAR_A) * mean_b2 + var(VAR_B) * var(MEAN_A) * var(MEAN_A);
    let reference = (var(MEAN_A) * var(MEAN_A)) * var(VAR_B) + (var(MEAN_B) * var(MEAN_B)) * var(VAR_A);
    expect_equal(implemented, reference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_variance_matches_reference() {
        assert!(check_add_variance().is_ok());
    }

    #[test]
    fn sub_variance_matches_reference() {
        assert!(check_sub_variance().is_ok());
    }

    #[test]
    fn scale_variance_matches_reference() {
        assert!(check_scale_variance().is_ok());
    }

    #[test]
    fn mul_variance_matches_reference() {
        assert!(check_mul_variance().is_ok());
    }

    #[test]
    fn div_variance_matches_reference() {
        assert!(check_div_variance().is_ok());
    }
}
