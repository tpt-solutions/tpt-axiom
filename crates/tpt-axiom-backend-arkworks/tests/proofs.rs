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
    from_bytes, to_bytes,
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

#[zk_provable(backend = "arkworks")]
/// A narrow-typed circuit: `small` is a `u8`, so anything outside `[0, 256)`
/// must be rejected even though the field would happily hold it.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn narrow_range(#[public] small: u8, #[secret] bump: u8) {
    assert!(small >= bump);
}

#[zk_provable(backend = "arkworks")]
/// The division gadget through the R1CS lowering: the quotient node itself
/// is unconstrained; the quotient/remainder identity and range checks tie
/// it down.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn divide(#[public] x: u64, #[public] quotient: u64, #[secret] d: u64) {
    assert!(d >= 1);
    assert_eq!(x / d, quotient);
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

/// Silences arkworks' `assert!(cs.is_satisfied())` panic output for the
/// negative-path tests below.
fn silence_panics() {
    static SILENCE: std::sync::Once = std::sync::Once::new();
    SILENCE.call_once(|| std::panic::set_hook(Box::new(|_| {})));
}

/// Real Groth16 proving on the given witness.
///
/// arkworks *asserts* `cs.is_satisfied()` inside its prover rather than
/// returning an error, so an out-of-range witness surfaces as a panic; this
/// helper normalises that into `Ok(false)`.
fn prove_succeeds(
    ir: &ConstraintSystem,
    public: &[i64],
    secret: &[i64],
) -> Result<ArkworksProof, ArkworksError> {
    let backend = ArkworksBackend;
    let circuit = backend.compile(ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(ir, &[]).expect("keys");
    silence_panics();
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        backend.prove(&circuit, &pk, public, secret)
    }));
    // arkworks panics on an unsatisfiable witness; report it as a failure.
    attempt.unwrap_or(Err(ArkworksError::Witness(WitnessError::Violated {
        index: 0,
    })))
}

#[test]
fn narrow_unsigned_inputs_are_range_checked() {
    let ir = NarrowRange.build();
    // 300 fits comfortably in the field but not in a `u8`: the bit sum can
    // never agree with the input, so the R1CS is unsatisfiable and no proof
    // exists.
    assert!(
        prove_succeeds(&ir, &[300], &[1]).is_err(),
        "a u8 input outside [0, 256) must not produce a proof"
    );
}

#[test]
fn narrow_unsigned_inputs_accept_in_range_values() {
    let ir = NarrowRange.build();
    assert!(
        prove_succeeds(&ir, &[200], &[1]).is_ok(),
        "200 is a valid u8"
    );
}

#[test]
fn negative_value_for_unsigned_input_is_rejected() {
    let ir = NarrowRange.build();
    assert!(
        prove_succeeds(&ir, &[-5], &[-9]).is_err(),
        "a negative value must not produce a proof for an unsigned input"
    );
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

#[test]
fn inputs_outside_declared_types_are_diagnosed() {
    // 300 fits the field but not a `u8`: the pre-prove witness check names
    // the offending input instead of leaving synthesis to fail opaquely.
    let err = compiled(&NarrowRange.build())
        .with_witness(vec![300], vec![1])
        .expect_err("300 is not a u8");
    assert!(matches!(err, WitnessError::InputOutOfRange { .. }));
    compiled(&NarrowRange.build())
        .with_witness(vec![200], vec![1])
        .expect("200 is a valid u8");
}

#[test]
fn verify_with_wrong_public_count_is_a_clean_false() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    assert!(
        !backend.verify(&vk, &[50], &proof).expect("clean false"),
        "one public too few must not verify"
    );
    assert!(
        !backend
            .verify(&vk, &[50, 20, 7], &proof)
            .expect("clean false"),
        "one public too many must not verify"
    );
}

#[test]
fn key_material_roundtrips_through_canonical_bytes() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let (_pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // A verifier can be handed only the serialized key.
    let vk_bytes = to_bytes(&vk).expect("encode vk");
    let decoded: VerifyingKey<Bls12_381> = from_bytes(&vk_bytes).expect("decode vk");
    assert_eq!(decoded, vk);

    // Trailing bytes mean a truncated or concatenated artifact; accepting the
    // prefix would verify against a different key than the file describes.
    let mut padded = vk_bytes;
    padded.push(0);
    assert!(
        from_bytes::<VerifyingKey<Bls12_381>>(&padded).is_err(),
        "trailing bytes must be refused, not ignored"
    );
}

#[test]
fn proof_envelope_roundtrips_and_stays_bound() {
    use tpt_axiom_zk::{ProofClaim, ir_digest};

    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");

    let claim = ProofClaim::new("prove_balance_transfer", &ir, &[50, 20], proof);
    let envelope = claim
        .to_envelope(&backend)
        .expect("groth16 proofs serialize");
    assert_eq!(envelope.backend, "arkworks");
    assert_eq!(envelope.binding, ir_digest(&ir));

    let wire = serde_json::to_string(&envelope).expect("serialize");
    let decoded: tpt_axiom_zk::ProofEnvelope = serde_json::from_str(&wire).expect("deserialize");
    let rebuilt = ProofClaim::from_envelope(&backend, &decoded).expect("decode");
    assert_eq!(rebuilt, claim);
    assert!(
        rebuilt.verify_with(&backend, &vk, &ir).expect("verify"),
        "the envelope-ported proof must still verify"
    );
}

#[test]
fn division_gadget_roundtrips_and_rejects_forged_quotients() {
    let backend = ArkworksBackend;
    let ir = Divide.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // 100 / 7 = 14, remainder 2: proves and verifies.
    let proof = backend
        .prove(&circuit, &pk, &[100, 14], &[7])
        .expect("prove");
    assert!(backend.verify(&vk, &[100, 14], &proof).expect("verify"));

    // Forged quotient 15 implies a negative remainder.
    assert!(backend.prove(&circuit, &pk, &[100, 15], &[7]).is_err());
    // Zero divisor: the gadget's range constraints fail.
    assert!(backend.prove(&circuit, &pk, &[100, 14], &[0]).is_err());
}

#[test]
fn overflow_bound_circuit_is_rejected_at_keygen() {
    // Four chained 64-bit muls reach 256 static bits — past any proving
    // field. Groth16 setup must refuse it instead of emitting keys for a
    // circuit whose field semantics are unfaithful.
    let mut b = ConstraintSystemBuilder::new("too_wide");
    let a = b.public_input("a");
    let c = b.secret_input("c");
    let m1 = b.mul(a, c);
    let m2 = b.mul(m1, c);
    let m3 = b.mul(m2, c);
    b.constrain_non_negative(m3);
    let ir = b.build();
    let backend = ArkworksBackend;
    let err = backend.generate_keys(&ir, &[]).unwrap_err();
    assert!(
        matches!(err, ArkworksError::InvalidCircuit(_)),
        "unexpected error: {err}"
    );
}

// --- Gadgets end to end: `!=`, `if`, bounded `for`, solved selectors ---

#[zk_provable(backend = "arkworks")]
#[allow(clippy::let_and_return, clippy::bool_to_int_with_if)] // kept-fn shape
fn arkworks_gadgets(#[public] cutoff: i64, #[secret] score: i64) -> i64 {
    assert!(score != cutoff);
    let mut acc = 0;
    for i in 1..4 {
        acc += i * score;
    }
    // The accumulator is proved, not just computed: 1x + 2x + 3x = 6x.
    assert_eq!(acc, 6 * score);
    let label = if score >= cutoff { 1 } else { 0 };
    label
}

#[test]
fn gadget_circuit_proves_with_solved_selectors() {
    use tpt_axiom_zk::{KeygenOptions, keygen, prove_named, verify_claim};
    let backend = tpt_axiom_backend_arkworks::ArkworksBackend;
    let (pk, vk) = keygen(&backend, &ArkworksGadgets, KeygenOptions::defaults()).expect("keygen");

    // score = 6, cutoff = 5: the positive arm of every gadget.
    let witness = ArkworksGadgetsInputs::new(5, 6).named().expect("named");
    let witness = ArkworksGadgetsInputs::with_output(witness, 1);
    let claim = prove_named(&backend, &ArkworksGadgets, &pk, &witness).expect("prove");
    assert!(
        verify_claim(&backend, &ArkworksGadgets, &vk, &claim).expect("verify"),
        "the solved-selector Groth16 proof must verify"
    );

    // The mirrored branch: score below cutoff, label = 0.
    let witness = ArkworksGadgetsInputs::new(5, 2).named().expect("named");
    let witness = ArkworksGadgetsInputs::with_output(witness, 0);
    let claim = prove_named(&backend, &ArkworksGadgets, &pk, &witness).expect("prove");
    assert!(verify_claim(&backend, &ArkworksGadgets, &vk, &claim).expect("verify"));

    // score == cutoff: refused by the `!=` gadget before proving.
    let false_witness = ArkworksGadgetsInputs::new(5, 5).named().expect("named");
    let false_witness = ArkworksGadgetsInputs::with_output(false_witness, 0);
    assert!(
        prove_named(&backend, &ArkworksGadgets, &pk, &false_witness).is_err(),
        "a false inequality must not produce a claim"
    );
}
