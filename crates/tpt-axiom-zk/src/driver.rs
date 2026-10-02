//! The typed prove/verify driver: circuit definition + named witness in,
//! [`ProofClaim`] out.
//!
//! Every call site so far had to hand-assemble the same five steps: build the
//! IR, key the witness values by name, project them positionally, compile,
//! prove, and wrap the proof in a claim committed to that IR. The
//! `#[zk_provable]`-generated `Inputs` type now does exactly that through
//! [`prove_named`], so a prover is three lines and — more importantly — every
//! prover takes the same checked path.
//!
//! Verification stays with [`ProofClaim::verify_with`], which re-derives the
//! IR digest before consulting the backend: a claim can never be replayed
//! against a weakened redefinition of the circuit.

use alloc::string::{String, ToString};

use tpt_axiom_ir::ConstraintSystem;

use crate::claim::ProofClaim;
use crate::named::{NamedWitness, WitnessValues};
use crate::{CircuitDefinition, ZkBackend};

/// Why a typed prove did not produce a claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProveError {
    /// The name-keyed witness does not match the circuit (a missing, unknown,
    /// or constraint-violating value).
    Witness(String),
    /// The circuit's IR is malformed — see
    /// [`ConstraintSystem::validate`].
    Circuit(String),
    /// The backend failed while compiling, key-generating, or proving.
    Backend(String),
}

impl core::fmt::Display for ProveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Witness(msg) => write!(f, "invalid witness: {msg}"),
            Self::Circuit(msg) => write!(f, "invalid circuit: {msg}"),
            Self::Backend(msg) => write!(f, "backend failure: {msg}"),
        }
    }
}

impl core::error::Error for ProveError {}

/// Compiles `definition`, proves `witness` against `pk`, and returns a
/// [`ProofClaim`] committed to the exact IR that was proved.
///
/// The witness is resolved and constraint-checked first, so a prover is
/// refused with a precise diagnosis before any proving work happens, rather
/// than producing a well-formed proof of a false statement for a downstream
/// verifier to reject.
///
/// # Errors
///
/// * [`ProveError::Circuit`] — the IR fails `validate`.
/// * [`ProveError::Witness`] — the named witness is incomplete, names an
///   unknown input, or violates the circuit's constraints.
/// * [`ProveError::Backend`] — the backend failed to compile, key-generate, or
///   prove.
pub fn prove_named<B, D>(
    backend: &B,
    definition: &D,
    pk: &B::ProvingKey,
    witness: &NamedWitness,
) -> Result<ProofClaim<B>, ProveError>
where
    B: ZkBackend,
    D: CircuitDefinition,
{
    let ir = definition.build();
    prove_ir_named(backend, definition.name(), &ir, pk, witness)
}

/// [`prove_named`] against an already-built IR, for callers that hold the
/// [`ConstraintSystem`] rather than the definition (a CLI reading an IR from
/// disk, say).
///
/// # Errors
/// As [`prove_named`].
pub fn prove_ir_named<B: ZkBackend>(
    backend: &B,
    circuit_name: &str,
    ir: &ConstraintSystem,
    pk: &B::ProvingKey,
    witness: &NamedWitness,
) -> Result<ProofClaim<B>, ProveError> {
    ir.validate()
        .map_err(|e| ProveError::Circuit(e.to_string()))?;
    let values: WitnessValues = witness
        .resolve_and_check(ir)
        .map_err(|e| ProveError::Witness(e.to_string()))?;
    let compiled = backend
        .compile(ir)
        .map_err(|e| ProveError::Backend(e.to_string()))?;
    let proof = backend
        .prove(&compiled, pk, values.public(), &values.secret_with_aux())
        .map_err(|e| ProveError::Backend(e.to_string()))?;
    Ok(ProofClaim::new(circuit_name, ir, values.public(), proof))
}

/// Verifies `claim` against `definition` with `backend` and `vk`.
///
/// This is [`ProofClaim::verify_with`] under a name that reads correctly at a
/// call site holding a definition rather than an IR. The IR-digest binding
/// still applies: a claim produced against a different circuit verifies as a
/// clean `Ok(false)`.
///
/// # Errors
/// Backend-specific verification failure. A claim for the wrong circuit, or a
/// malformed proof, is `Ok(false)` rather than an error.
pub fn verify_claim<B, D>(
    backend: &B,
    definition: &D,
    vk: &B::VerifyingKey,
    claim: &ProofClaim<B>,
) -> Result<bool, B::Error>
where
    B: ZkBackend,
    D: CircuitDefinition,
{
    claim.verify_with(backend, vk, &definition.build())
}

/// Generates keys for `definition` with typed [`KeygenOptions`](crate::KeygenOptions).
///
/// # Errors
/// Backend-specific key-generation failure, or an invalid circuit.
pub fn keygen<B, D>(
    backend: &B,
    definition: &D,
    options: crate::options::KeygenOptions,
) -> Result<(B::ProvingKey, B::VerifyingKey), B::Error>
where
    B: ZkBackend,
    D: CircuitDefinition,
{
    backend.generate_keys_with_options(&definition.build(), &options)
}
