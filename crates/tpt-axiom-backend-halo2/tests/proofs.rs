//! `halo2` backend tests: `MockProver` circuit checks plus real IPA
//! prove/verify roundtrips. The test circuits are defined with the real
//! `#[zk_provable]` macro so the whole pipeline (Rust, IR, `PLONKish`, proof)
//! is exercised.

use tpt_axiom_backend_halo2::{
    Halo2Backend, Halo2Circuit, Halo2Error, WitnessError, auto_k, encode_scalar,
};
use tpt_axiom_ir::ConstraintSystem;
use tpt_axiom_ir::ConstraintSystemBuilder;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

#[zk_provable(backend = "halo2")]
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

#[zk_provable(backend = "halo2")]
/// Test circuit mirroring the shared conformance IR.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn bounds_check(#[secret] x: i64, #[public] lo: i64, #[public] hi: i64) {
    assert!(x >= lo);
    assert!(x <= hi);
    assert!(x < hi + 1);
}

#[zk_provable(backend = "halo2")]
/// Test circuit: public output from a secret multiplier.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn weighted_sum(a: u64, b: u64, #[secret] k: u64) -> u64 {
    let scaled = a * k;
    scaled + b
}

#[zk_provable(backend = "halo2")]
/// A narrow-typed circuit: `small` is a `u8`, so anything outside `[0, 256)`
/// must be rejected even though the field would happily hold it.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn narrow_range(#[public] small: u8, #[secret] bump: u8) {
    assert!(small >= bump);
}

#[zk_provable(backend = "halo2")]
/// The division gadget: a public quotient over a secret divisor, proven
/// through the quotient/remainder identity and the remainder's range.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn divide(#[public] x: u64, #[public] quotient: u64, #[secret] d: u64) {
    assert!(d >= 1);
    assert_eq!(x / d, quotient);
}

#[zk_provable(backend = "halo2")]
/// The remainder half of the gadget.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn remainder_of(#[public] x: u64, #[public] rem: u64, #[secret] d: u64) {
    assert!(d >= 1);
    assert_eq!(x % d, rem);
}

#[zk_provable(backend = "halo2")]
/// Verifiable uncertain claims (Phase D): the published fused estimate is the
/// minimum-variance fusion of two secret readings (within a ±1-unit rounding
/// window) and clears `threshold`. Means in milli-units, variances in
/// micro-units².
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_fused_estimate(
    #[secret] mean_a: i64,
    #[secret] var_a: i64,
    #[secret] mean_b: i64,
    #[secret] var_b: i64,
    #[public] reported_mean: i64,
    #[public] reported_variance: i64,
    #[public] threshold: i64,
) {
    assert!(var_a >= 1 && var_b >= 1);
    let v_sum = var_a + var_b;
    let mean_num = mean_a * var_b + mean_b * var_a;
    let mean_err = reported_mean * v_sum - mean_num;
    assert!(mean_err + v_sum >= 0);
    assert!(v_sum - mean_err >= 0);
    let var_err = reported_variance * v_sum - var_a * var_b;
    assert!(var_err + v_sum >= 0);
    assert!(v_sum - var_err >= 0);
    assert!(reported_mean >= threshold);
}

#[allow(clippy::missing_const_for_fn)] // trivial test helper
fn compiled(ir: &ConstraintSystem) -> Halo2Circuit {
    Halo2Circuit::compile(ir, 64, 0)
}

#[allow(clippy::missing_const_for_fn)] // trivial test helper
fn balance_transfer(public: [i64; 2], secret: [i64; 1]) -> Halo2Circuit {
    compiled(&ProveBalanceTransfer.build())
        .with_witness_unchecked(public.to_vec(), secret.to_vec())
        .expect("arity")
}

#[allow(clippy::missing_const_for_fn)] // trivial test helper
fn bounds(public: [i64; 2], secret: [i64; 1]) -> Halo2Circuit {
    compiled(&BoundsCheck.build())
        .with_witness_unchecked(public.to_vec(), secret.to_vec())
        .expect("arity")
}

fn checked(ir: &ConstraintSystem, public: Vec<i64>, secret: Vec<i64>) -> WitnessError {
    compiled(ir)
        .with_witness(public, secret)
        .expect_err("witness must be rejected")
}

/// Runs the `MockProver` over a witnessed circuit; public instance values
/// come from the witness itself.
fn mock_check(circuit: &Halo2Circuit) -> Result<(), Vec<halo2_proofs::dev::VerifyFailure>> {
    let public = circuit.public().expect("witnessed circuit");
    let instance: Vec<_> = public.iter().map(|&v| encode_scalar(v)).collect();
    let k = auto_k(&circuit.ir, circuit.range_bits);
    halo2_proofs::dev::MockProver::run(k, circuit, vec![instance])
        .expect("MockProver::run")
        .verify()
}

#[test]
fn mock_prover_accepts_valid_witness() {
    let circuit = balance_transfer([50, 20], [30]);
    mock_check(&circuit).expect("valid witness must satisfy the circuit");
}

#[test]
fn mock_prover_rejects_overdraft() {
    // amount = 100 > sender = 50 violates `sender >= amount`: the PLONKish
    // NonNegative range check must fail even for an unchecked witness.
    let circuit = balance_transfer([50, 20], [100]);
    assert!(
        mock_check(&circuit).is_err(),
        "overdraft must fail the circuit"
    );
    assert_eq!(
        checked(&ProveBalanceTransfer.build(), vec![50, 20], vec![100]),
        WitnessError::Violated { index: 0 }
    );
}

#[test]
fn mock_prover_accepts_negative_secrets() {
    // `x = -5` inside `[lo, hi] = [-10, 10]`: signed input range checks must
    // admit negative i64 witnesses.
    let circuit = bounds([-10, 10], [-5]);
    mock_check(&circuit).expect("negative in-bounds secret must pass");
}

#[test]
fn mock_prover_rejects_out_of_bounds_secret() {
    let circuit = bounds([0, 10], [11]);
    assert!(
        mock_check(&circuit).is_err(),
        "x > hi must fail the circuit"
    );
    assert_eq!(
        checked(&BoundsCheck.build(), vec![0, 10], vec![11]),
        WitnessError::Violated { index: 1 }
    );
}

#[test]
fn mock_prover_rejects_extreme_secret() {
    // amount = i64::MIN is not a `u64` at all: exact `i128` witness checking
    // rejects it up front with the declared-type diagnosis (and the circuit's
    // own range check would still reject it, as `mock_check` shows).
    let circuit = balance_transfer([50, 20], [i64::MIN]);
    assert!(mock_check(&circuit).is_err(), "wrapping witness must fail");
    assert!(matches!(
        checked(&ProveBalanceTransfer.build(), vec![50, 20], vec![i64::MIN]),
        WitnessError::InputOutOfRange { .. }
    ));
}

#[test]
fn wrapping_intermediate_is_rejected_at_prove_time() {
    // `sender - amount` under exact arithmetic spans up to 2^64 - 1 — the
    // exact last value a 64-bit range check admits — so the widest honest
    // signed transfer proves fine.
    let mut b = ConstraintSystemBuilder::new("signed_transfer");
    let sender = b.public_input_typed("sender", tpt_axiom_ir::IntType::I64);
    let amount = b.secret_input_typed("amount", tpt_axiom_ir::IntType::I64);
    let surplus = b.sub(sender, amount);
    b.constrain_non_negative(surplus);
    compiled(&b.build())
        .with_witness(vec![i64::MAX], vec![i64::MIN])
        .expect("2^64 - 1 is the last admitted NonNegative value");
    // A product blows past the range-check width while staying
    // non-negative; the witness check rejects it with the precise range
    // diagnosis instead of an opaque synthesis failure.
    let mut b = ConstraintSystemBuilder::new("wide_product");
    let x = b.public_input_typed("x", tpt_axiom_ir::IntType::I64);
    let y = b.secret_input_typed("y", tpt_axiom_ir::IntType::I64);
    let p = b.mul(x, y);
    b.constrain_non_negative(p);
    let err = compiled(&b.build())
        .with_witness(vec![1i64 << 40], vec![1i64 << 30])
        .expect_err("2^70 exceeds the 64-bit range check");
    assert!(matches!(err, WitnessError::NonNegativeOutOfRange { .. }));
}

#[test]
fn zero_constraints_are_enforced() {
    // Hand-built IR exercising the `Zero` constraint branch, which the macro
    // never emits on its own.
    let mut b = ConstraintSystemBuilder::new("zero_check");
    let x = b.public_input("x");
    let y = b.secret_input("y");
    let diff = b.sub(x, y);
    b.constrain_zero(diff);
    let base = compiled(&b.build());

    let good = base.with_witness(vec![9], vec![9]).expect("arity");
    mock_check(&good).expect("x == y must satisfy the zero constraint");

    let bad = base
        .with_witness_unchecked(vec![9], vec![8])
        .expect("arity");
    assert!(mock_check(&bad).is_err(), "x != y must violate the circuit");
    assert_eq!(
        checked(&base.ir, vec![9], vec![8]),
        WitnessError::Violated { index: 0 }
    );
}

#[test]
fn real_proof_roundtrip() {
    let backend = Halo2Backend;
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
    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    // Same proof, different public inputs: must not verify.
    assert!(
        !backend
            .verify(&vk, &[50, 21], &proof)
            .expect("verify should yield a clean false"),
        "tampered public inputs must fail verification"
    );
}

#[test]
fn real_proof_rejects_violating_secret() {
    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let result = backend.prove(&circuit, &pk, &[50, 20], &[100]);
    assert!(result.is_err(), "overdraft secret must fail at prove time");
    assert!(
        matches!(
            result.unwrap_err(),
            tpt_axiom_backend_halo2::Halo2Error::Witness(WitnessError::Violated { index: 0 })
        ),
        "the violated NonNegative constraint must be reported"
    );
}

#[test]
fn real_proof_signed_roundtrip() {
    let backend = Halo2Backend;
    let ir = BoundsCheck.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[-10, 10], &[-5])
        .expect("prove with negative values");
    assert!(backend.verify(&vk, &[-10, 10], &proof).expect("verify"));
}

#[test]
fn real_proof_public_output_roundtrip() {
    // weighted_sum has a public return value: 2*4 + 3 = 11.
    let backend = Halo2Backend;
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
fn narrow_unsigned_inputs_are_range_checked() {
    let ir = NarrowRange.build();
    // 300 fits comfortably in the field but not in a `u8`.
    let circuit = compiled(&ir)
        .with_witness_unchecked(vec![300], vec![1])
        .expect("arity");
    assert!(
        mock_check(&circuit).is_err(),
        "a u8 input outside [0, 256) must not satisfy the circuit"
    );
}

#[test]
fn narrow_unsigned_inputs_accept_in_range_values() {
    let ir = NarrowRange.build();
    let circuit = compiled(&ir)
        .with_witness_unchecked(vec![200], vec![1])
        .expect("arity");
    mock_check(&circuit).expect("200 is a valid u8");
}

#[test]
fn negative_value_for_unsigned_input_is_rejected() {
    let ir = NarrowRange.build();
    // A negative witness cannot be represented as an unsigned `bits`-wide
    // value, so the bit decomposition cannot sum back to the input.
    let circuit = compiled(&ir)
        .with_witness_unchecked(vec![-5], vec![-9])
        .expect("arity");
    assert!(
        mock_check(&circuit).is_err(),
        "a negative value must not satisfy an unsigned input"
    );
}

#[test]
fn witness_arity_is_checked() {
    let base = compiled(&ProveBalanceTransfer.build());
    assert!(matches!(
        base.with_witness(vec![50], vec![30]),
        Err(WitnessError::Arity { kind: "public", .. })
    ));
    assert!(matches!(
        base.with_witness(vec![50, 20], vec![30, 1]),
        Err(WitnessError::Arity { kind: "secret", .. })
    ));
}

#[test]
fn violated_witness_produces_diagnostic() {
    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let err = backend.prove(&circuit, &pk, &[50, 20], &[100]).unwrap_err();
    let rendered = err.to_string();
    assert!(
        rendered.contains("constraint"),
        "unexpected message: {rendered}"
    );
}

#[test]
fn verify_with_wrong_public_count_is_a_clean_false() {
    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    // One public too few and one too many are both false claims, not
    // backend errors.
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
fn proof_envelope_roundtrips_and_stays_bound() {
    use tpt_axiom_zk::{ProofClaim, ir_digest};

    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");

    let claim = ProofClaim::new("prove_balance_transfer", &ir, &[50, 20], proof);
    // Envelope: the serializable port of the claim.
    let envelope = claim.to_envelope(&backend).expect("halo2 proofs are bytes");
    assert_eq!(envelope.version, tpt_axiom_zk::ENVELOPE_VERSION);
    assert_eq!(envelope.backend, "halo2");
    assert_eq!(envelope.binding, ir_digest(&ir));
    // Rebuild through the (JSON) wire format and verify in the *decoded*
    // claim — the digest binding survives.
    let wire = serde_json::to_string(&envelope).expect("serialize");
    let decoded: tpt_axiom_zk::ProofEnvelope = serde_json::from_str(&wire).expect("deserialize");
    let rebuilt = ProofClaim::from_envelope(&backend, &decoded).expect("decode");
    assert_eq!(rebuilt, claim);
    assert!(
        rebuilt.verify_with(&backend, &vk, &ir).expect("verify"),
        "the envelope-portaled proof must still verify"
    );
    // A claim rebuilt against a DIFFERENT circuit is refused.
    let mut other_ir = ProveBalanceTransfer.build();
    other_ir.constraints.clear();
    assert!(
        !rebuilt
            .verify_with(&backend, &vk, &other_ir)
            .expect("clean false"),
        "digest mismatch must refuse the claim"
    );
    // A foreign envelope (wrong backend name) decodes to nothing.
    let mut foreign = decoded;
    foreign.backend = String::from("arkworks");
    assert!(ProofClaim::from_envelope(&backend, &foreign).is_none());
}

#[test]
fn verifiable_fusion_claim_accepts_honest_readings() {
    // A = 10.500 ± 0.200, B = 10.700 ± 0.300 in fixed point; published
    // fusion rounded to the milli-unit grid.
    let secrets = [10_500_i64, 40_000_i64, 10_700_i64, 90_000_i64];
    let publics = [10_562_i64, 27_692_i64, 10_000_i64];
    let backend = Halo2Backend;
    let ir = ProveFusedEstimate.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &publics, &secrets)
        .expect("prove");
    assert!(
        backend.verify(&vk, &publics, &proof).expect("verify"),
        "the honest fusion claim must verify"
    );
    // The verifier learns the fusion — and nothing about which reading was
    // which: swapped secrets describe the same published estimate.
    let swapped = [secrets[2], secrets[3], secrets[0], secrets[1]];
    let proof = backend
        .prove(&circuit, &pk, &publics, &swapped)
        .expect("prove");
    assert!(backend.verify(&vk, &publics, &proof).expect("verify"));
}

#[test]
fn verifiable_fusion_claim_rejects_forgery() {
    let secrets = [10_500_i64, 40_000_i64, 10_700_i64, 90_000_i64];
    let publics = [10_562_i64, 27_692_i64, 10_000_i64];
    let backend = Halo2Backend;
    let ir = ProveFusedEstimate.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // Mis-stated fusion: 0.24 units off, far outside the rounding window.
    let mut forged = publics;
    forged[0] += 240;
    assert!(matches!(
        backend.prove(&circuit, &pk, &forged, &secrets),
        Err(Halo2Error::Witness(WitnessError::Violated { .. }))
    ));
    // Below-threshold claim: inside the window, outside the policy.
    let mut low = publics;
    low[0] = 10_550;
    low[2] = 10_600;
    assert!(backend.prove(&circuit, &pk, &low, &secrets).is_err());
    // Degenerate zero variance: rejected up front, before proving.
    let mut zeroed = secrets;
    zeroed[1] = 0;
    assert!(matches!(
        backend.prove(&circuit, &pk, &publics, &zeroed),
        Err(Halo2Error::Witness(WitnessError::Violated { index: 0 }))
    ));
}

#[test]
fn division_gadget_roundtrips_and_rejects_forged_quotients() {
    let backend = Halo2Backend;
    let ir = Divide.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // 100 / 7 = 14 with remainder 2: the honest quotient proves and verifies.
    let proof = backend
        .prove(&circuit, &pk, &[100, 14], &[7])
        .expect("prove");
    assert!(backend.verify(&vk, &[100, 14], &proof).expect("verify"));

    // The forged quotient 15 implies remainder -5, outside [0, d-1].
    let err = backend
        .prove(&circuit, &pk, &[100, 15], &[7])
        .expect_err("a forged quotient must not prove");
    assert!(matches!(
        err,
        Halo2Error::Witness(WitnessError::Violated { .. })
    ));

    // A zero divisor fails the gadget's range constraints — division by
    // zero cannot prove.
    assert!(backend.prove(&circuit, &pk, &[100, 14], &[0]).is_err());
}

#[test]
fn remainder_gadget_roundtrips() {
    let backend = Halo2Backend;
    let ir = RemainderOf.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[100, 2], &[7])
        .expect("prove");
    assert!(backend.verify(&vk, &[100, 2], &proof).expect("verify"));
    // Wrong remainder: 3 implies quotient 13.857…, outside the integers.
    assert!(backend.prove(&circuit, &pk, &[100, 3], &[7]).is_err());
}

#[test]
fn overflow_bound_circuit_is_rejected_at_compile() {
    // Four chained 64-bit muls reach 256 static bits — past any proving
    // field. Key generation must refuse it instead of emitting a key for a
    // circuit whose field semantics are unfaithful.
    let mut b = ConstraintSystemBuilder::new("too_wide");
    let a = b.public_input("a");
    let c = b.secret_input("c");
    let m1 = b.mul(a, c);
    let m2 = b.mul(m1, c);
    let m3 = b.mul(m2, c);
    b.constrain_non_negative(m3);
    let ir = b.build();
    let backend = Halo2Backend;
    let err = backend.generate_keys(&ir, &[]).unwrap_err();
    assert!(
        matches!(err, tpt_axiom_backend_halo2::Halo2Error::InvalidCircuit(_)),
        "unexpected error: {err}"
    );
}

// --- Gadgets end to end: `!=`, `if`, bounded `for`, solved selectors ---

#[zk_provable(backend = "halo2")]
#[allow(clippy::let_and_return, clippy::bool_to_int_with_if)] // kept-fn shape
fn halo2_gadgets(#[public] cutoff: i64, #[secret] score: i64) -> i64 {
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
    let backend = Halo2Backend;
    let (pk, vk) = keygen(&backend, &Halo2Gadgets, KeygenOptions::defaults()).expect("keygen");

    // score = 6, cutoff = 5: 6 ≠ 5, the branch selects, acc = 6+12+18 = 36,
    // label = 1. The prover names only the declared inputs — every selector
    // is solved inside the driver.
    let witness = Halo2GadgetsInputs::new(5, 6).named().expect("named");
    let witness = Halo2GadgetsInputs::with_output(witness, 1);
    let claim = prove_named(&backend, &Halo2Gadgets, &pk, &witness).expect("prove");
    assert!(
        verify_claim(&backend, &Halo2Gadgets, &vk, &claim).expect("verify"),
        "the solved-selector proof must verify"
    );

    // The mirrored branch (score below cutoff) proves too: label = 0 with
    // the selector on the other arm.
    let witness = Halo2GadgetsInputs::new(5, 2).named().expect("named");
    let witness = Halo2GadgetsInputs::with_output(witness, 0);
    let claim = prove_named(&backend, &Halo2Gadgets, &pk, &witness).expect("prove");
    assert!(verify_claim(&backend, &Halo2Gadgets, &vk, &claim).expect("verify"));

    // score == cutoff: the `!=` gadget is unsatisfiable in either arm, so
    // the driver refuses before any proving work.
    let false_witness = Halo2GadgetsInputs::new(5, 5).named().expect("named");
    let false_witness = Halo2GadgetsInputs::with_output(false_witness, 0);
    assert!(
        prove_named(&backend, &Halo2Gadgets, &pk, &false_witness).is_err(),
        "a false inequality must not produce a claim"
    );
}
