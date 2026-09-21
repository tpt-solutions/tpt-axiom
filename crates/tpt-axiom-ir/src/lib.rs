//! # tpt-axiom-ir
//!
//! The shared arithmetic intermediate representation (IR) for `tpt-axiom`.
//!
//! This is the backend-agnostic gate/constraint format that the
//! [`#[zk_provable]`](https://docs.rs/tpt-axiom-macros) macro lowers Rust
//! functions into. It sits between the Rust source (application logic) and any
//! concrete zero-knowledge backend (`halo2`, `arkworks`, `sp1`).
//!
//! The IR is intentionally small:
//!
//! * an expression DAG built from [`ConstraintSystemBuilder`] operations
//!   (constants, variables, `+`, `-`, `*`, unary `-`);
//! * a set of [`Constraint`]s asserting equality, zero, or non-negativity;
//! * an optional lowering to [`R1CS`] quadratic constraints, proving the IR is
//!   expressive enough for the standard ZK constraint forms.
//!
//! For now the scalar "field" is modelled as [`Scalar`] = `i64`; backend
//! adapter crates translate IR expressions into their native field during
//! compilation (Phase 4).

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rust_2018_idioms)]

extern crate alloc;

pub mod circuit;
pub mod r1cs;

pub use circuit::{Constraint, ConstraintSystem, ConstraintSystemBuilder, Expr, VariableInfo, Visibility};
pub use r1cs::{evaluate_linear, EvaluationError, Linear, R1csAssertion, R1csGate, R1CS};

/// Integer scalar model used across the IR. Real backend adapters map this
/// type onto their native cryptographic field element.
pub type Scalar = i64;
