//! # axiom-backend-sp1
//!
//! The SP1 adapter for `tpt-axiom`.
//!
//! **Status: Phase 4 scaffolding.** This crate is intentionally an empty shell
//! until the backend work lands. It will implement
//! [`axiom_zk::ZkBackend`] for SP1, compiling the backend-agnostic
//! [`axiom_ir::ConstraintSystem`] into an SP1 program and generating real
//! proofs.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use axiom_ir;
pub use axiom_zk;

/// The SP1 [`axiom_zk::ZkBackend`] implementation (Phase 4).
#[derive(Debug, Default, Clone, Copy)]
pub struct Sp1Backend;

impl Sp1Backend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "sp1";
}