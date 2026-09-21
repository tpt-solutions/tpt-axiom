//! # axiom-zk
//!
//! Backend-agnostic zero-knowledge abstractions for `tpt-axiom`.
//!
//! Two traits sit at the seam between the Rust source and any concrete proving
//! system:
//!
//! * [`CircuitDefinition`] — implemented by the `#[zk_provable]` macro for
//!   every annotated function. Its [`CircuitDefinition::build`] lowers the
//!   function's constraints into an [`axiom_ir::ConstraintSystem`].
//! * [`ZkBackend`] — the trait adapter crates (`axiom-backend-halo2`,
//!   `axiom-backend-arkworks`, `axiom-backend-sp1`) implement to compile an IR
//!   circuit, generate keys, prove, and verify.
//!
//! No real backend is wired up yet (Phase 4); this crate defines the contract.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rust_2018_idioms)]

pub use axiom_ir;

mod backend;
mod circuit;

pub use crate::backend::ZkBackend;
pub use crate::circuit::CircuitDefinition;