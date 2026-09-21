//! Lowering the shared IR to backend-specific forms (R1CS, `PLONKish`).

use crate::ConstraintGraph;

/// A target form the [`ConstraintGraph`] IR can be lowered to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweringTarget {
    /// Rank-1 constraint systems (e.g. arkworks/groth16-style backends).
    R1cs,
    /// `PLONKish` arithmetization (e.g. halo2-style backends).
    Plonkish,
}

/// An error produced while lowering the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweringError {
    /// Lowering for this target is not implemented yet.
    NotYetImplemented,
}

impl core::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotYetImplemented => write!(f, "lowering not yet implemented"),
        }
    }
}

impl std::error::Error for LoweringError {}

/// Lower a [`ConstraintGraph`] to the given target form.
///
/// Phase 2 scaffold: real lowering logic lands with the `#[zk_provable]`
/// macro and backend adapters.
///
/// # Errors
/// Returns [`LoweringError::NotYetImplemented`] for every target; no
/// lowering backend exists yet.
pub const fn lower(_graph: &ConstraintGraph, target: LoweringTarget) -> Result<(), LoweringError> {
    match target {
        LoweringTarget::R1cs | LoweringTarget::Plonkish => Err(LoweringError::NotYetImplemented),
    }
}
