//! The backend-agnostic constraint graph.

use crate::{Constraint, Gate};

/// The kind of a wire in a [`ConstraintGraph`]: whether it is a public input,
/// a secret witness, or an internal/output value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireKind {
    /// A public input, visible to prover and verifier.
    Public,
    /// A secret witness value, known only to the prover.
    Secret,
    /// An internal or output wire computed from other wires.
    Internal,
}

/// A handle to a wire (value slot) in a [`ConstraintGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Wire(pub usize);

/// An index into a [`ConstraintGraph`]'s constraint list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstraintIndex(pub usize);

/// A backend-agnostic arithmetic constraint graph: the IR that
/// `#[zk_provable]` lowers Rust functions into, and that `ZkBackend`
/// implementations compile into concrete circuits.
///
/// Phase 2 scaffold: this is the minimal structure needed to represent a
/// graph of wires/gates/constraints; lowering and macro-driven construction
/// land with the rest of Phase 2.
#[derive(Debug, Clone, Default)]
pub struct ConstraintGraph {
    wires: Vec<WireKind>,
    gates: Vec<Gate>,
    constraints: Vec<Constraint>,
}

impl ConstraintGraph {
    /// Create an empty constraint graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new wire of the given kind, returning its handle.
    pub fn alloc_wire(&mut self, kind: WireKind) -> Wire {
        self.wires.push(kind);
        Wire(self.wires.len() - 1)
    }

    /// Append a gate to the graph.
    pub fn push_gate(&mut self, gate: Gate) {
        self.gates.push(gate);
    }

    /// Append a constraint to the graph, returning its index.
    pub fn push_constraint(&mut self, constraint: Constraint) -> ConstraintIndex {
        self.constraints.push(constraint);
        ConstraintIndex(self.constraints.len() - 1)
    }

    /// The kind of a given wire.
    #[must_use]
    pub fn wire_kind(&self, wire: Wire) -> WireKind {
        self.wires[wire.0]
    }

    /// All gates currently in the graph.
    #[must_use]
    pub fn gates(&self) -> &[Gate] {
        &self.gates
    }

    /// All constraints currently in the graph.
    #[must_use]
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wires_and_gates_round_trip() {
        let mut graph = ConstraintGraph::new();
        let a = graph.alloc_wire(WireKind::Public);
        let b = graph.alloc_wire(WireKind::Secret);
        let out = graph.alloc_wire(WireKind::Internal);
        graph.push_gate(Gate::Add {
            lhs: a,
            rhs: b,
            out,
        });
        graph.push_constraint(Constraint::Equal(out, out));

        assert_eq!(graph.wire_kind(a), WireKind::Public);
        assert_eq!(graph.wire_kind(b), WireKind::Secret);
        assert_eq!(graph.gates().len(), 1);
        assert_eq!(graph.constraints().len(), 1);
    }
}
