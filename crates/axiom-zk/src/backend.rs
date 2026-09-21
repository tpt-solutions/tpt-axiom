//! The [`ZkBackend`] trait implemented by adapter crates.

use core::fmt::Display;

use axiom_ir::{ConstraintSystem, R1CS, Scalar};

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
    type Proof;

    /// Stable identifier, e.g. `"halo2"`, `"arkworks"`, `"sp1"`.
    fn name(&self) -> &'static str;

    /// Compile an IR circuit into the backend's native circuit form.
    fn compile(&self, ir: &ConstraintSystem) -> Result<Self::Circuit, Self::Error>;

    /// Lower the IR into the reference R1CS form (used for conformance and the
    /// `tpt-telos` equivalence bridge).
    fn lower_r1cs(&self, ir: &ConstraintSystem) -> R1CS {
        axiom_ir::r1cs::lower_r1cs(ir)
    }

    /// Generate proving and verifying keys for an IR circuit. `params` carries
    /// backend-specific security parameters (e.g. the K t from halo2); an empty
    /// slice requests the backend's defaults.
    fn generate_keys(
        &self,
        ir: &ConstraintSystem,
        params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error>;

    /// Produce a proof for an assignment of the circuit's inputs.
    ///
    /// `public` and `secret` hold the values of `ir.public_inputs` and
    /// `ir.secret_inputs` respectively, in declaration order.
    fn prove(
        &self,
        circuit: &Self::Circuit,
        pk: &Self::ProvingKey,
        public: &[Scalar],
        secret: &[Scalar],
    ) -> Result<Self::Proof, Self::Error>;

    /// Verify a proof against the public inputs only.
    fn verify(
        &self,
        vk: &Self::VerifyingKey,
        public: &[Scalar],
        proof: &Self::Proof,
    ) -> Result<bool, Self::Error>;
}