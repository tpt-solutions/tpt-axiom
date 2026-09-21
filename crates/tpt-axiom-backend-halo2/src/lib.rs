//! # tpt-axiom-backend-halo2
//!
//! The `halo2` adapter for `tpt-axiom`.
//!
//! **Status: Phase 4 scaffolding.** This crate is intentionally an empty shell
//! until the backend work lands. It will implement
//! [`tpt_axiom_zk::ZkBackend`] for `halo2`, generating real proving and verifying
//! keys at build time and producing proofs that verify independently of the
//! proving process.
//!
//! The adapter receives the backend-agnostic [`tpt_axiom_ir::ConstraintSystem`]
//! produced by `#[zk_provable]` and compiles it into halo2's constraint form
//! (PLONKish gates); the reference R1CS lowering in `tpt-axiom-ir` defines the
//! semantics it must agree with.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use tpt_axiom_ir;
pub use tpt_axiom_zk;

/// The halo2 [`tpt_axiom_zk::ZkBackend`] implementation (Phase 4).
#[derive(Debug, Default, Clone, Copy)]
pub struct Halo2Backend;

impl Halo2Backend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "halo2";
}
