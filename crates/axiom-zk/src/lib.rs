//! # axiom-zk
//!
//! Backend-agnostic zero-knowledge abstractions for tpt-axiom: the
//! [`ZkBackend`] trait plus the circuit / proving-key / verifying-key
//! contract that `axiom-backend-*` crates implement (Phase 4) and that
//! `#[zk_provable]` (Phase 2) targets.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use axiom_ir::ConstraintGraph;

/// A zero-knowledge proving backend (e.g. halo2, arkworks, sp1).
///
/// Phase 2 scaffold: this defines the contract; concrete implementations
/// land with the backend adapter crates in Phase 4.
pub trait ZkBackend {
    /// The backend's compiled circuit representation.
    type Circuit;
    /// The backend's proving key.
    type ProvingKey;
    /// The backend's verifying key.
    type VerifyingKey;
    /// The backend's witness assignment.
    type Witness;
    /// The backend's proof.
    type Proof;
    /// An error produced by this backend.
    type Error: std::error::Error;

    /// Compile a backend-agnostic constraint graph into this backend's
    /// circuit representation.
    ///
    /// # Errors
    /// Returns `Self::Error` if the graph cannot be compiled for this
    /// backend.
    fn compile(graph: &ConstraintGraph) -> Result<Self::Circuit, Self::Error>;

    /// Generate a witness for `circuit` from concrete input values.
    ///
    /// # Errors
    /// Returns `Self::Error` if the inputs don't satisfy the circuit.
    fn witness(circuit: &Self::Circuit, inputs: &[u64]) -> Result<Self::Witness, Self::Error>;

    /// Generate a proving key for `circuit`.
    ///
    /// # Errors
    /// Returns `Self::Error` if key generation fails.
    fn setup_proving_key(circuit: &Self::Circuit) -> Result<Self::ProvingKey, Self::Error>;

    /// Generate a verifying key for `circuit`.
    ///
    /// # Errors
    /// Returns `Self::Error` if key generation fails.
    fn setup_verifying_key(circuit: &Self::Circuit) -> Result<Self::VerifyingKey, Self::Error>;

    /// Produce a proof from a proving key and witness.
    ///
    /// # Errors
    /// Returns `Self::Error` if proof generation fails.
    fn prove(pk: &Self::ProvingKey, witness: &Self::Witness) -> Result<Self::Proof, Self::Error>;

    /// Verify a proof against a verifying key and public inputs.
    ///
    /// # Errors
    /// Returns `Self::Error` if verification cannot be performed (as opposed
    /// to a valid verification that simply returns `Ok(false)`).
    fn verify(
        vk: &Self::VerifyingKey,
        proof: &Self::Proof,
        public_inputs: &[u64],
    ) -> Result<bool, Self::Error>;
}
