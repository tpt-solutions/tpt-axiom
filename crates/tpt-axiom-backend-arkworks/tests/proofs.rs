//! Arkworks backend tests: real Groth16 (BLS12-381) key generation, proving
//! and verification, rejection paths, and canonical serialization. The test
//! circuits are defined with the real `#[zk_provable]` macro so the whole
//! pipeline (Rust → IR → R1CS → proof) is exercised.

use ark_bls12_381::Bls12_381;
use ark_groth16::{Groth16, ProvingKey, VerifyingKey};
use ark_relations::r1cs::ConstraintSynthesizer;
use ark_serialize::CanonicalDeserialize;
use ark_snark::SNARK;
use tpt_axiom_backend_arkworks::{
    ArkworksBackend, ArkworksCircuit, ArkworksError, ArkworksProof, WitnessError, encode_scalar,
    to_bytes,
};
use tpt_axiom_ir::{ConstraintSystem, ConstraintSystemBuilder};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

#[zk_provable(backend = "arkworks")]
/// Test circuit mirroring the shared conformance IR.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

#[zk_provable(backend = "arkworks")]
/// Test circuit mirroring the shared conformance IR.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn bounds_check(#[secret] x: i64, #[public] lo: i64, #[public] hi: i64) {
    assert!(x >= lo);
    assert!(x <= hi);
    assert!(x < hi + 1);
}

#[zk_provable(backend = "arkworks")]
/// Test circuit: public output from a secret multiplier.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn weighted_sum(a: u64, b: u64, #[secret] k: u64) -> u64 {
    let scaled = a * k;
    scaled + b
}

#[allow(clippy::missing_const_for_fn)] // trivial test helper
fn compiled(ir: &ConstraintSystem) -> ArkworksCircuit {
    ArkworksCircuit::compile(ir, 64)
}

/// Generates a real Groth16 proof, bypassing the IR-level witness pre-check.
fn prove_unchecked(
    pk: &ProvingKey<Bls12_381>,
    circuit: &ArkworksCircuit,
    public: &[i64],
    secret: &[i64],
) -> Result<ArkworksProof, ArkworksError> {
    let witnessed = circuit.with_witness_unchecked(public.to_vec(), secret.to_vec())?;
    Ok(ArkworksProof(Groth16::<Bls12_381>::prove(
        pk,
        witnessed,
        &mut ark_std::rand::rngs::OsRng,
    )?))
}

#[test]
fn constraint_count_is_sane() {
    let ir = ProveBalanceTransfer.build();
    let circuit = compiled(&ir);
    let cs = ark_relations::r1cs::ConstraintSystem::<ark_bls12_381::Fr>::new_ref();
    // Setup mode: synthesize the constraint structure without witnesses.
    cs.set_mode(ark_relations::r1cs::SynthesisMode::Setup);
    circuit.generate_constraints(cs.clone()).unwrap();
    let count = cs.num_constraints();
    // 4 range checks x (64 booleanity + 1 sum) + 3 op-node gates + 1 Equal.
    assert!(
        (250..300).contains(&count),
        "unexpected constraint count {count}"
    );
}

#[test]
fn real_proof_roundtrip() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    assert!(
        backend.verify(&vk, &[50, 20], &proof).expect("verify"),
        "valid proof must verify"
    );
}

#[test]
fn real_proof_rejects_tampered_publics() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    assert!(
        !backend
            .verify(&vk, &[50, 21], &proof)
            .expect("verify yields a clean false"),
        "tampered public inputs must fail verification"
    );
}

#[test]
fn real_proof_rejects_violating_secret() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // The IR-level check rejects with a precise diagnosis...
    let checked = backend.prove(&circuit, &pk, &[50, 20], &[100]);
    assert!(matches!(
        checked.unwrap_err(),
        ArkworksError::Witness(WitnessError::Violated { index: 0 })
    ));

    // ...and even bypassing the check, R1CS synthesis refuses the false
    // witness: arkworks' prover asserts `cs.is_satisfied()` and panics rather
    // than emitting a proof of a false statement.
    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prove_unchecked(&pk, &circuit, &[50, 20], &[100])
    }));
    assert!(raw.is_err(), "R1CS must refuse a false witness");
}

#[test]
fn real_proof_signed_roundtrip() {
    let backend = ArkworksBackend;
    let ir = BoundsCheck.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[-10, 10], &[-5])
        .expect("prove with negative values");
    assert!(backend.verify(&vk, &[-10, 10], &proof).expect("verify"));
}

#[test]
fn real_proof_rejects_out_of_bounds_secret() {
    let backend = ArkworksBackend;
    let ir = BoundsCheck.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");
    assert!(
        backend.prove(&circuit, &pk, &[0, 10], &[11]).is_err(),
        "x > hi must fail at prove time"
    );
}

#[test]
fn real_proof_public_output_roundtrip() {
    // weighted_sum has a public return value: 2*4 + 3 = 11.
    let backend = ArkworksBackend;
    let ir = WeightedSum.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[2, 3, 11], &[4])
        .expect("prove");
    assert!(backend.verify(&vk, &[2, 3, 11], &proof).expect("verify"));

    // Wrong claimed output must not verify.
    assert!(
        !backend
            .verify(&vk, &[2, 3, 12], &proof)
            .expect("clean false"),
        "wrong public output must fail"
    );
}

#[test]
fn keys_and_proofs_roundtrip_through_bytes() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");

    let pk_bytes = to_bytes(&pk).expect("pk serialize");
    let vk_bytes = to_bytes(&vk).expect("vk serialize");
    let proof_bytes = to_bytes(&proof.0).expect("proof serialize");
    assert!(!pk_bytes.is_empty() && !vk_bytes.is_empty() && !proof_bytes.is_empty());

    let vk_back = VerifyingKey::<Bls12_381>::deserialize_compressed(vk_bytes.as_slice())
        .expect("vk deserialize");
    let proof_back =
        ark_groth16::Proof::<Bls12_381>::deserialize_compressed(proof_bytes.as_slice())
            .expect("proof deserialize");
    let inputs: Vec<_> = [50i64, 20].iter().map(|&v| encode_scalar(v)).collect();
    assert!(
        Groth16::<Bls12_381>::verify(&vk_back, &inputs, &proof_back).expect("verify shipped keys"),
        "proof must verify against the deserialized verifying key"
    );
}

#[test]
fn zero_constraints_are_enforced() {
    let mut b = ConstraintSystemBuilder::new("zero_check");
    let x = b.public_input("x");
    let y = b.secret_input("y");
    let diff = b.sub(x, y);
    b.constrain_zero(diff);
    let ir = b.build();
    let backend = ArkworksBackend;

    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    let good = backend.prove(&circuit, &pk, &[9], &[9]).expect("prove");
    assert!(backend.verify(&vk, &[9], &good).expect("verify"));

    assert!(
        backend.prove(&circuit, &pk, &[9], &[8]).is_err(),
        "x != y must violate the zero constraint"
    );
}

#[test]
fn witness_arity_is_checked() {
    let base = compiled(&ProveBalanceTransfer.build());
    assert!(matches!(
        base.with_witness_unchecked(vec![50], vec![30]),
        Err(WitnessError::Arity { kind: "public", .. })
    ));
    assert!(matches!(
        base.with_witness_unchecked(vec![50, 20], vec![30, 1]),
        Err(WitnessError::Arity { kind: "secret", .. })
    ));
}
