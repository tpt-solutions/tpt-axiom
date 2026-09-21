//! Arithmetic gates lowered from Rust expressions.

use crate::graph::Wire;

/// A single arithmetic gate connecting input wires to an output wire.
///
/// Phase 2 scaffold: gate kinds will grow to cover the arithmetic subset
/// supported by `#[zk_provable]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `out = lhs + rhs`.
    Add {
        /// Left-hand input wire.
        lhs: Wire,
        /// Right-hand input wire.
        rhs: Wire,
        /// Output wire.
        out: Wire,
    },
    /// `out = lhs - rhs`.
    Sub {
        /// Left-hand input wire.
        lhs: Wire,
        /// Right-hand input wire.
        rhs: Wire,
        /// Output wire.
        out: Wire,
    },
    /// `out = lhs * rhs`.
    Mul {
        /// Left-hand input wire.
        lhs: Wire,
        /// Right-hand input wire.
        rhs: Wire,
        /// Output wire.
        out: Wire,
    },
}
