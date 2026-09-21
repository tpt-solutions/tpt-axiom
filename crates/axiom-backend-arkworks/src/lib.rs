//! # axiom-backend-arkworks
//!
//! The `arkworks` adapter for `tpt-axiom`.
//!
//! **Status: Phase 4 scaffolding.** This crate is intentionally an empty shell
//! until the backend work lands. It will implement
//! [`axiom_zk::ZkBackend`] for the arkworks proving stack, converting the
//! backend-agnostic [`axiom_ir::ConstraintSystem`] into arkworks R1CS.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use axiom_ir;
pub use axiom_zk;

/// The arkworks [`axiom_zk::ZkBackend`] implementation (Phase 4).
#[derive(Debug, Default, Clone, Copy)]
pub struct ArkworksBackend;

impl ArkworksBackend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "arkworks";
}