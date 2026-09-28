//! The verification boundary between probabilistic computation and
//! zero-knowledge proof.
//!
//! A [`ProofClaim`] is the *statement* "this named circuit holds for these
//! public inputs" together with an opaque proof of it. It is deliberately
//! data-only: the intelligence types in `tpt-axiom-core` never reference it,
//! so cryptographic mechanisms remain opt-in and cannot contaminate the core
//! type system. Verification runs through any [`ZkBackend`] — the claim never
//! sees keys or circuits itself.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ZkBackend;
use tpt_axiom_ir::Scalar;

/// A portable, verifiable statement: `circuit` holds for `publics`, witnessed
/// by an opaque backend-specific `proof`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofClaim<B: ZkBackend> {
    circuit: String,
    publics: Vec<Scalar>,
    proof: B::Proof,
}

impl<B: ZkBackend> ProofClaim<B> {
    /// Records a claim that `circuit` holds for `publics` with `proof`.
    #[must_use]
    pub fn new(circuit: impl Into<String>, publics: &[Scalar], proof: B::Proof) -> Self {
        Self {
            circuit: circuit.into(),
            publics: publics.to_vec(),
            proof,
        }
    }

    /// The claimed circuit name.
    #[must_use]
    pub fn circuit(&self) -> &str {
        &self.circuit
    }

    /// The claimed public inputs, in the circuit's declaration order.
    #[must_use]
    pub fn publics(&self) -> &[Scalar] {
        &self.publics
    }

    /// Verifies this claim with `backend` against the circuit's verifying
    /// key.
    ///
    /// This is the one-way boundary: probabilistic code can hold a claim and
    /// hand it to any backend for checking, without the claim (or anything
    /// upstream of it) depending on the backend's types.
    ///
    /// # Errors
    /// Backend-specific verification failure. A malformed proof is a clean
    /// `Ok(false)` where the backend can express that.
    pub fn verify_with(&self, backend: &B, vk: &B::VerifyingKey) -> Result<bool, B::Error> {
        backend.verify(vk, &self.publics, &self.proof)
    }
}
