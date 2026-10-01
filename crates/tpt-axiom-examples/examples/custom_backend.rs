//! A skeleton `ZkBackend` for adapter authors: the full trait contract
//! implemented against the IR's reference R1CS lowering — **with zero
//! cryptography**.
//!
//! `SkeletonBackend` proves nothing: its "proof" is the witness vector
//! itself, and `verify` re-evaluates the constraint system against it. That
//! is obviously not sound (the proof *is* the secret), and it says so
//! everywhere — its purpose is the shape:
//!
//! 1. `compile` lowers the IR once (`self.lower_r1cs(ir)` is the trait's
//!    provided method — the reference R1CS form lives in `tpt-axiom-ir`);
//! 2. `generate_keys` validates and derives whatever material the prover
//!    and verifier need;
//! 3. `prove` checks the witness against the IR *before* doing the
//!    expensive work (every real backend should fail here, not after);
//! 4. `verify` consumes only the verifying key, the public inputs, and the
//!    proof — never the secrets.
//!
//! A real adapter replaces step 3's witness evaluation with actual proving
//! (halo2: `PLONKish` gates + IPA; arkworks: R1CS + Groth16) and step 4's
//! re-evaluation with a pairing or an inner-product check.
//!
//! Run it:
//!
//! ```sh
//! cargo run -p tpt-axiom-zk --example custom_backend
//! ```
//!
//! Expected output:
//!
//! ```text
//! compiled: 6 nodes -> 4 gates, 2 public + 1 secret slots
//! keys: R1csKeys { constraints: 4 }
//! proved with witness [50, 20, 30, ..]; proof verifies: true
//! dishonest witness rejected at prove time: witness violates the circuit's constraint #0
//! conformance: this skeleton satisfies the prove/verify contract on the
//!   balance-transfer shape; wire tpt_axiom_zk::conformance's drivers into
//!   your adapter's test suite for the full battery
//! ```

use tpt_axiom_ir::r1cs::{EvaluationError, R1CS};
use tpt_axiom_ir::{ConstraintSystem, ConstraintSystemBuilder, Scalar};
use tpt_axiom_zk::{ZkBackend, witness};

/// The skeleton backend: R1CS evaluation where a real proving stack would
/// go. **Not sound** — see the module docs.
#[derive(Debug, Default, Clone, Copy)]
pub struct SkeletonBackend;

/// The R1CS with a filled witness vector. In a sound backend this would be
/// a succinct proof instead; here the witness *is* the artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessProof {
    /// Witness slots, `witness[0] = 1`.
    witness: Vec<Scalar>,
}

/// Key material of the skeleton backend (the lowered R1CS itself).
#[derive(Clone, Debug)]
pub struct R1csKeys {
    /// The evaluated constraint system (same object for both sides in the
    /// skeleton; a real backend's keys are cryptographic material).
    system: R1CS,
}

impl core::fmt::Display for R1csKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "R1csKeys {{ constraints: {} }}", self.system.gates.len())
    }
}

/// Witness-shape failures re-exported through the backend error.
#[derive(Debug)]
pub enum SkeletonError {
    /// The IR was rejected by `ConstraintSystem::validate`.
    InvalidCircuit(tpt_axiom_ir::CircuitError),
    /// The witness was rejected before proving.
    Witness(witness::WitnessError),
    /// Constraint evaluation failed (only reachable through a tampered
    /// proof, since `prove` pre-checks).
    Eval(EvaluationError),
}

impl core::fmt::Display for SkeletonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCircuit(e) => write!(f, "invalid circuit: {e}"),
            Self::Witness(e) => write!(f, "witness mismatch: {e}"),
            Self::Eval(e) => write!(f, "constraint evaluation failed: {e}"),
        }
    }
}

impl std::error::Error for SkeletonError {}

impl From<witness::WitnessError> for SkeletonError {
    fn from(e: witness::WitnessError) -> Self {
        Self::Witness(e)
    }
}

impl From<tpt_axiom_ir::CircuitError> for SkeletonError {
    fn from(e: tpt_axiom_ir::CircuitError) -> Self {
        Self::InvalidCircuit(e)
    }
}

/// The lowered circuit: the reference R1CS *plus* the IR.
///
/// Filling the witness requires evaluating the expression DAG; a native
/// backend's circuit type would carry whatever its prover needs instead.
#[derive(Clone, Debug)]
pub struct SkeletonCircuit {
    ir: ConstraintSystem,
    r1cs: R1CS,
}

impl ZkBackend for SkeletonBackend {
    type Error = SkeletonError;
    type Circuit = SkeletonCircuit;
    type ProvingKey = R1csKeys;
    type VerifyingKey = R1csKeys;
    type Proof = WitnessProof;

    fn name(&self) -> &'static str {
        "skeleton"
    }

    fn compile(&self, ir: &ConstraintSystem) -> Result<Self::Circuit, Self::Error> {
        // The trait provides the reference R1CS lowering; a native backend
        // would translate into its own constraint form here instead.
        Ok(SkeletonCircuit {
            ir: ir.clone(),
            r1cs: self.lower_r1cs(ir).map_err(SkeletonError::InvalidCircuit)?,
        })
    }

    fn generate_keys(
        &self,
        ir: &ConstraintSystem,
        _params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error> {
        // Nothing secret to generate without cryptography — but this is
        // where a real backend runs its setup (halo2: structured params;
        // Groth16: the per-circuit trusted setup).
        let keys = R1csKeys {
            system: self.lower_r1cs(ir).map_err(SkeletonError::InvalidCircuit)?,
        };
        Ok((keys.clone(), keys))
    }

    fn prove(
        &self,
        circuit: &Self::Circuit,
        _pk: &Self::ProvingKey,
        public: &[Scalar],
        secret: &[Scalar],
    ) -> Result<Self::Proof, Self::Error> {
        // Fail before doing any work — the same discipline the real
        // backends keep.
        witness::check(&circuit.ir, public, secret)?;
        // Fill every expression slot bottom-up from the (already checked)
        // node values; intermediate values outside `i64` wrap here exactly
        // as the reference evaluator's slot model assumes.
        let nodes = witness::evaluate_nodes(&circuit.ir, Some(public), Some(secret));
        let mut w = vec![1; circuit.r1cs.num_variables];
        for (node, slot) in circuit.r1cs.expr_slots.iter().enumerate() {
            if let Some(slot) = slot {
                #[allow(clippy::cast_possible_truncation)] // slot model is i64
                {
                    w[*slot] = nodes[node].unwrap_or(0) as Scalar;
                }
            }
        }
        w[0] = 1;
        circuit.r1cs.evaluate(&w).map_err(SkeletonError::Eval)?;
        Ok(WitnessProof { witness: w })
    }

    fn verify(
        &self,
        vk: &Self::VerifyingKey,
        public: &[Scalar],
        proof: &Self::Proof,
    ) -> Result<bool, Self::Error> {
        // Public inputs must match the proof's committed slots.
        let slots = &vk.system.public_slots;
        if public.len() != slots.len()
            || public
                .iter()
                .zip(slots)
                .any(|(&p, &slot)| proof.witness[slot] != p)
        {
            return Ok(false);
        }
        match vk.system.evaluate(&proof.witness) {
            Ok(()) => Ok(true),
            Err(EvaluationError::GateFailed { .. } | EvaluationError::NonNegativeFailed { .. }) => {
                Ok(false)
            }
            Err(other) => Err(SkeletonError::Eval(other)),
        }
    }
}

/// The balance-transfer demo IR, built inline (the shared conformance IR
/// lives behind the `conformance` feature).
fn balance_transfer_ir() -> ConstraintSystem {
    let mut b = ConstraintSystemBuilder::new("prove_balance_transfer");
    let sender = b.public_input("sender_balance");
    let receiver = b.public_input("receiver_balance");
    let amount = b.secret_input("amount");
    let surplus = b.sub(sender, amount);
    b.constrain_non_negative(surplus);
    let new_receiver = b.add(receiver, amount);
    let expected = b.add(receiver, amount);
    b.constrain_eq(new_receiver, expected);
    b.build()
}

fn main() {
    let backend = SkeletonBackend;
    let ir = balance_transfer_ir();
    let circuit = backend.compile(&ir).expect("compile");
    println!(
        "compiled: {} nodes -> {} gates, {} public + {} secret slots",
        ir.exprs.len(),
        circuit.r1cs.gates.len(),
        circuit.r1cs.public_slots.len(),
        circuit.r1cs.secret_slots.len(),
    );
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    println!("keys: {vk}");

    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("honest witness must prove");
    let ok = backend.verify(&vk, &[50, 20], &proof).expect("verdict");
    println!(
        "proved with witness [50, 20, 30, ..]; proof verifies: {ok}"
    );
    assert!(ok);

    let err = backend
        .prove(&circuit, &pk, &[50, 20], &[100])
        .expect_err("an overdraft must not prove");
    println!("dishonest witness rejected at prove time: {err}");

    println!(
        "conformance: this skeleton satisfies the prove/verify contract on the\n  balance-transfer shape; wire tpt_axiom_zk::conformance's drivers into\n  your adapter's test suite for the full battery"
    );
}
