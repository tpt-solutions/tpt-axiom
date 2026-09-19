//! # axiom-ir
//!
//! A shared, backend-agnostic **arithmetic intermediate representation** for
//! tpt-axiom.  It is what [`crate`](https://github.com/tpt-solutions/tpt-axiom)'s
//! `#[zk_provable]` macro lowers Rust functions *into*, and what verification
//! tooling (e.g. `tpt-telos`, Phase 3) checks for **circuit equivalence** with
//! the original Rust logic.
//!
//! The IR is a constraint graph that can be lowered to R1CS or `PLONKish` (halo2)
//! forms.  It carries **variance-propagation constraints** (Phase 1 types) and
//! arithmetic gate/constraint nodes (Phase 2).
//!
//! ## Phase 2 scope
//!
//! - Constraint graph (`ConstraintGraph`, [`Gate`], [`Constraint`])
//! - Public/secret input classification
//! - Lowering to R1CS and `PLONKish` forms
//!
//! This crate is verified formally in Phase 3; a **circuit-mismatch detector**
//! fails the build when Rust logic and generated circuit diverge.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod constraint;
pub mod gate;
pub mod graph;
pub mod lowering;

pub use constraint::Constraint;
pub use gate::Gate;
pub use graph::{ConstraintGraph, ConstraintIndex, Wire, WireKind};
