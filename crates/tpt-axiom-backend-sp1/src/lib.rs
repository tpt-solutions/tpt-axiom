//! # tpt-axiom-backend-sp1
//!
//! The SP1 adapter slot for `tpt-axiom`.
//!
//! **Status: contract scaffolded, proving backend deferred.** SP1 is a zkVM:
//! unlike halo2/arkworks (which compile the [`tpt_axiom_ir::ConstraintSystem`]
//! into a constraint system), SP1 executes a RISC-V program inside a ZK
//! virtual machine. A faithful adapter therefore does not lower the IR into
//! gates; it would emit a small RISC-V program that interprets the IR's
//! expression table and constrains its evaluation, then prove that program's
//! execution with `sp1_sdk`.
//!
//! That work is blocked in this environment: the SP1 toolchain
//! (`sp1-sdk`, plus the `riscv32im-succinct-zkvm-elf` Rust target installed
//! via `cargo prove install-toolchain`) is only distributed for Linux and
//! macOS, so the program and its proofs cannot be built or executed here.
//! The [`Sp1Backend`] type below implements the full [`tpt_axiom_zk::ZkBackend`]
//! contract so the shape of the adapter is fixed and dependents compile;
//! every proving operation returns [`Sp1Error::Unavailable`] until the SP1
//! stack lands. See `todo.md`'s Phase 4 for the follow-up.
//!
//! The IR-to-program lowering should live in a future
//! `tpt-axiom-sp1-program` crate (host-side IR encoder + guest-side
//! interpreter), keeping `sp1-sdk` out of this crate's dependency tree until
//! then.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use core::fmt;

pub use tpt_axiom_ir;
pub use tpt_axiom_zk;

/// The SP1 [`tpt_axiom_zk::ZkBackend`] implementation.
///
/// The contract is fully implemented; every operation that would require the
/// SP1 toolchain reports [`Sp1Error::Unavailable`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Sp1Backend;

impl Sp1Backend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "sp1";
}

/// Failures of the SP1 backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sp1Error {
    /// The SP1 proving stack is not available in this build.
    ///
    /// SP1's SDK and RISC-V toolchain are Linux/macOS-only today; see the
    /// crate documentation for the planned adapter design.
    Unavailable,
    /// The IR circuit is not well-formed (the same bar the real backends
    /// enforce at compile time).
    InvalidCircuit(String),
}

impl fmt::Display for Sp1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(
                f,
                "the SP1 backend is deferred: the SP1 SDK and riscv32im-succinct-zkvm-elf \
                 toolchain are not distributable on this platform (Linux/macOS only); \
                 see tpt-axiom-backend-sp1 documentation"
            ),
            Self::InvalidCircuit(msg) => write!(f, "invalid circuit: {msg}"),
        }
    }
}

impl std::error::Error for Sp1Error {}

/// Marker circuit handle produced by
/// [`Sp1Backend::compile`](tpt_axiom_zk::ZkBackend::compile).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sp1Circuit(pub alloc::sync::Arc<tpt_axiom_ir::ConstraintSystem>);

/// Marker proving key type.
#[derive(Clone, Debug, Default)]
pub struct Sp1ProvingKey;

/// Marker verifying key type.
#[derive(Clone, Debug, Default)]
pub struct Sp1VerifyingKey;

/// Marker proof type.
#[derive(Clone, Debug, Default)]
pub struct Sp1Proof;

impl tpt_axiom_zk::ZkBackend for Sp1Backend {
    type Error = Sp1Error;
    type Circuit = Sp1Circuit;
    type ProvingKey = Sp1ProvingKey;
    type VerifyingKey = Sp1VerifyingKey;
    type Proof = Sp1Proof;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn compile(&self, ir: &tpt_axiom_ir::ConstraintSystem) -> Result<Self::Circuit, Self::Error> {
        // The same well-formedness bar as the real backends: an IR whose
        // structure or static bit widths are unfaithful over a field is
        // rejected here too, so swapping backends never changes the verdict.
        ir.validate()
            .map_err(|e| Sp1Error::InvalidCircuit(e.to_string()))?;
        // Compilation into an SP1 program is what the deferred lowering will
        // provide; until then the IR itself is retained as the circuit handle.
        Ok(Sp1Circuit(alloc::sync::Arc::new(ir.clone())))
    }

    fn generate_keys(
        &self,
        _ir: &tpt_axiom_ir::ConstraintSystem,
        _params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error> {
        Err(Sp1Error::Unavailable)
    }

    fn prove(
        &self,
        _circuit: &Self::Circuit,
        _pk: &Self::ProvingKey,
        _public: &[tpt_axiom_ir::Scalar],
        _secret: &[tpt_axiom_ir::Scalar],
    ) -> Result<Self::Proof, Self::Error> {
        Err(Sp1Error::Unavailable)
    }

    fn verify(
        &self,
        _vk: &Self::VerifyingKey,
        _public: &[tpt_axiom_ir::Scalar],
        _proof: &Self::Proof,
    ) -> Result<bool, Self::Error> {
        Err(Sp1Error::Unavailable)
    }
}
