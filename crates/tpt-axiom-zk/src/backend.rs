//! The [`ZkBackend`] trait implemented by adapter crates.

use alloc::vec::Vec;
use core::fmt::Display;

use tpt_axiom_ir::{CircuitError, ConstraintSystem, R1CS, Scalar};

use crate::options::KeygenOptions;

/// A zero-knowledge proving backend (halo2, arkworks, sp1, …).
///
/// Implementations translate an [`ConstraintSystem`] into the backend's native
/// circuit representation, generate proving/verifying keys, produce proofs, and
/// verify them independently of the proving process.
///
/// The trait is backend-agnostic: the same [`ConstraintSystem`] (built by
/// `#[zk_provable]`) is fed to every adapter, and the backend conformance
/// suite (Phase 4) runs identical example circuits through all of them.
pub trait ZkBackend {
    /// Backend-specific failure type.
    type Error: Display;
    /// The backend's compiled circuit / witness-assignment object.
    type Circuit;
    /// Proving key produced at build time.
    type ProvingKey;
    /// Verifying key shipped to verifiers.
    type VerifyingKey;
    /// A produced proof.
    ///
    /// `Clone + Debug` because a proof is data: claims and audit bundles
    /// carry and log it.
    type Proof: Clone + core::fmt::Debug;

    /// Stable identifier, e.g. `"halo2"`, `"arkworks"`, `"sp1"`.
    fn name(&self) -> &'static str;

    /// Compile an IR circuit into the backend's native circuit form.
    ///
    /// # Errors
    /// Backend-specific failure while lowering the IR.
    fn compile(&self, ir: &ConstraintSystem) -> Result<Self::Circuit, Self::Error>;

    /// Lower the IR into the reference R1CS form (used for conformance and
    /// equivalence checking).
    ///
    /// # Errors
    /// [`CircuitError`] when the IR is not well-formed (see
    /// [`ConstraintSystem::validate`]).
    fn lower_r1cs(&self, ir: &ConstraintSystem) -> Result<R1CS, CircuitError> {
        tpt_axiom_ir::r1cs::lower_r1cs(ir)
    }

    /// Generate proving and verifying keys for an IR circuit. `params` carries
    /// backend-specific security parameters (e.g. the K t from halo2); an empty
    /// slice requests the backend's defaults.
    ///
    /// # Errors
    /// Backend-specific failure during parameter generation or key derivation.
    fn generate_keys(
        &self,
        ir: &ConstraintSystem,
        params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error>;

    /// [`KeygenOptions`] flavour of [`Self::generate_keys`]: the same call with
    /// validated, typed parameters.
    ///
    /// Backends get this for free — the options are encoded into the byte form
    /// the trait already speaks — so a backend only overrides it to accept
    /// parameters the blob cannot express.
    ///
    /// # Errors
    /// Backend-specific failure during parameter generation or key derivation.
    fn generate_keys_with_options(
        &self,
        ir: &ConstraintSystem,
        options: &KeygenOptions,
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error> {
        self.generate_keys(ir, &options.encode())
    }

    /// Produce a proof for an assignment of the circuit's inputs.
    ///
    /// `public` and `secret` hold the values of `ir.public_inputs` and
    /// `ir.secret_inputs` respectively, in declaration order.
    ///
    /// # Errors
    /// Backend-specific failure while synthesizing or proving; implementations
    /// are expected to reject witnesses that violate the IR.
    fn prove(
        &self,
        circuit: &Self::Circuit,
        pk: &Self::ProvingKey,
        public: &[Scalar],
        secret: &[Scalar],
    ) -> Result<Self::Proof, Self::Error>;

    /// Verify a proof against the public inputs only.
    ///
    /// # Errors
    /// Backend-specific failure while checking the proof. A malformed proof
    /// is a clean `Ok(false)`, not an error, where the backend can
    /// distinguish the two.
    fn verify(
        &self,
        vk: &Self::VerifyingKey,
        public: &[Scalar],
        proof: &Self::Proof,
    ) -> Result<bool, Self::Error>;

    /// Canonical bytes for a produced proof, when the backend supports
    /// proof serialization (see [`ProofEnvelope`](crate::ProofEnvelope)).
    /// `None` means the backend's proof type is not portable bytes.
    fn encode_proof(&self, _proof: &Self::Proof) -> Option<Vec<u8>> {
        None
    }

    /// Rebuilds a proof from [`Self::encode_proof`] bytes.
    /// `None` means the bytes are not a proof of this backend.
    fn decode_proof(&self, _bytes: &[u8]) -> Option<Self::Proof> {
        None
    }
}
