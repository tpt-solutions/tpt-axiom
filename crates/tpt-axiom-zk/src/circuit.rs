//! The [`CircuitDefinition`] trait implemented by `#[zk_provable]` output.

use tpt_axiom_ir::ConstraintSystem;

/// A function that has been turned into a zero-knowledge circuit.
///
/// The `#[zk_provable(backend = "...")]` macro generates one implementation of
/// this trait per annotated function. `build` re-derives the function's
/// arithmetic constraints as a backend-agnostic [`ConstraintSystem`].
pub trait CircuitDefinition {
    /// Stable human-readable circuit name (matches the annotated function).
    fn name(&self) -> &'static str;

    /// The backend selected via `#[zk_provable(backend = "…")]`.
    fn backend(&self) -> &'static str;

    /// Lower the function to the arithmetic IR.
    fn build(&self) -> ConstraintSystem;
}