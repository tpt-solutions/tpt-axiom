//! # tpt-axiom-zk
//!
//! Backend-agnostic zero-knowledge abstractions for `tpt-axiom`.
//!
//! Two traits sit at the seam between the Rust source and any concrete proving
//! system:
//!
//! * [`CircuitDefinition`] — implemented by the `#[zk_provable]` macro for
//!   every annotated function. Its [`CircuitDefinition::build`] lowers the
//!   function's constraints into an [`tpt_axiom_ir::ConstraintSystem`].
//! * [`ZkBackend`] — the trait adapter crates (`tpt-axiom-backend-halo2`,
//!   `tpt-axiom-backend-arkworks`, `tpt-axiom-backend-sp1`) implement to compile
//!   an IR circuit, generate keys, prove, and verify.
//!
//! Around those sit the caller-facing pieces: [`named::NamedWitness`] builds a
//! witness by input *name* and refuses to resolve it onto the positional form
//! unless it is complete, [`layout::InputLayout`] prints what a verifier sees,
//! [`options::KeygenOptions`] types the key-generation parameters, and
//! [`driver`] ties it together as `prove_named` / `verify_claim`.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rust_2018_idioms)]

extern crate alloc;

pub use tpt_axiom_ir;

mod backend;
mod circuit;
pub mod claim;
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod driver;
pub mod layout;
pub mod named;
pub mod options;
#[cfg(feature = "registry")]
pub mod registry;
pub mod witness;

/// Re-exported so a `#[zk_provable(..., register)]` circuit needs no `linkme`
/// dependency of its own — the generated code points `#[linkme(crate = ...)]`
/// here.
#[cfg(feature = "registry")]
pub use linkme;

pub use crate::backend::ZkBackend;
pub use crate::circuit::CircuitDefinition;
pub use crate::claim::{ENVELOPE_VERSION, ProofClaim, ProofEnvelope, ir_digest};
pub use crate::driver::{ProveError, keygen, prove_ir_named, prove_named, verify_claim};
pub use crate::layout::{InputLayout, InputSlot};
pub use crate::named::{NamedWitness, NamedWitnessError, WitnessValues};
pub use crate::options::{KeygenOptions, KeygenOptionsError};
#[cfg(feature = "registry")]
pub use crate::registry::{REGISTERED_CIRCUITS, RegisteredCircuit};
