//! Proof-carrying decisions: a [`DecisionRecord`] bundled with the
//! [`ProofClaim`] that justifies it.
//!
//! [`tpt_axiom_core::DecisionRecord`] answers *who decided what, with what
//! confidence, and where it came from*. [`ProofCarriedDecision`] adds *and
//! here is a verifiable proof of the statement the decision was computed
//! from* — the claim's IR-digest binding means the proof stays tied to the
//! exact circuit it was produced against, so an audit replays the
//! verification against the same definition, not a lookalike.
//!
//! The split of responsibilities is deliberate: the proof attests the
//! *statement* (e.g. "balance ≥ amount" for these public inputs); the
//! decision — the threshold policy that turned a score into a commit — is
//! recorded alongside it and remains the caller's policy, never re-derived
//! here.

use tpt_axiom_core::DecisionRecord;
use tpt_axiom_ir::Scalar;
use tpt_axiom_zk::{ProofClaim, ZkBackend};

/// A [`DecisionRecord`] carrying the [`ProofClaim`] that justifies it.
#[derive(Clone, Debug)]
pub struct ProofCarriedDecision<T, B: ZkBackend> {
    record: DecisionRecord<T>,
    claim: ProofClaim<B>,
}

impl<T, B: ZkBackend> ProofCarriedDecision<T, B> {
    /// Bundles a decision record with its justifying claim.
    #[must_use]
    pub const fn new(record: DecisionRecord<T>, claim: ProofClaim<B>) -> Self {
        Self { record, claim }
    }

    /// The decision and its provenance.
    #[must_use]
    pub const fn record(&self) -> &DecisionRecord<T> {
        &self.record
    }

    /// The justifying claim (statement + public inputs + opaque proof).
    #[must_use]
    pub const fn claim(&self) -> &ProofClaim<B> {
        &self.claim
    }
}

impl<T, B: ZkBackend> ProofCarriedDecision<T, B> {
    /// Re-verifies the carried claim against `ir` — the circuit definition
    /// the auditor believes it is checking — with `backend` and `vk`.
    ///
    /// Returns `Ok(false)` when the proof is invalid *or* the claim was
    /// produced against a different circuit ([`ProofClaim::verify_with`]'s
    /// digest check), so an auditor can never wave through a proof made for
    /// a weakened definition.
    ///
    /// # Errors
    /// Backend-specific verification failure.
    pub fn verify(
        &self,
        backend: &B,
        vk: &B::VerifyingKey,
        ir: &tpt_axiom_ir::ConstraintSystem,
    ) -> Result<bool, B::Error> {
        self.claim.verify_with(backend, vk, ir)
    }

    /// The public inputs the proof was made over — the exact statement the
    /// decision was computed from, for the audit trail.
    #[must_use]
    pub fn claimed_publics(&self) -> &[Scalar] {
        self.claim.publics()
    }
}
