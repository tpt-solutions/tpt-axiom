//! Arithmetic constraints in the shared IR constraint graph.

use crate::graph::Wire;

/// A single arithmetic constraint between wires in a [`crate::ConstraintGraph`].
///
/// Phase 2 scaffold: constraint kinds will grow as the `#[zk_provable]` macro
/// and its lowering targets are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// `a == b`.
    Equal(Wire, Wire),
}
