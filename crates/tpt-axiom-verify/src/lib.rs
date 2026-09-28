//! # tpt-axiom-verify
//!
//! Polynomial-identity verification for `tpt-axiom`'s generated ZK circuits
//! and `Fuzzy<T>` variance-propagation formulas (Phase 3).
//!
//! Both `tpt-axiom-ir`'s expression DAG (constants, variables, `+`, `-`, `*`,
//! unary `-`) and `Fuzzy<T>`'s variance formulas are polynomials in their
//! inputs. Normalizing an expression into a canonical multivariate
//! [`poly::Polynomial`] gives *exact*, decidable equivalence checking —
//! including the bilinear/quadratic terms (e.g. `a * k` for two witnesses)
//! that a linear-arithmetic-only SMT solver cannot express — with no
//! external dependency.
//!
//! * [`circuit::check_comparison`] proves that a [`tpt_axiom_ir::Constraint`]
//!   in a [`tpt_axiom_ir::ConstraintSystem`] encodes the comparison it was
//!   claimed to (catches a `#[zk_provable]` lowering bug).
//! * [`circuit::evaluate`] concretely evaluates a `ConstraintSystem`, for
//!   cross-checking the IR against the kept-original Rust function on
//!   sampled inputs.
//! * [`fuzzy`] checks `Fuzzy<T>`'s variance-propagation formulas against
//!   their textbook identities (see that module's docs for exactly what this
//!   does and doesn't prove).

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rust_2018_idioms)]

extern crate alloc;

pub mod circuit;
pub mod fuzzy;
pub mod poly;

pub use circuit::{check_comparison, evaluate, normalize_all, Comparison};
pub use poly::{expect_equal, Mismatch, Polynomial};
