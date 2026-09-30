//! `halo2` backend tests: `MockProver` circuit checks plus real IPA
//! prove/verify roundtrips. The test circuits are defined with the real
//! `#[zk_provable]` macro so the whole pipeline (Rust, IR, `PLONKish`, proof)
//! is exercised.

use tpt_axiom_backend_halo2::{Halo2Backend, Halo2Circuit, WitnessError, auto_k, encode_scalar};
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
    // amount = i64::MIN makes `sender - amount` wrap in the i64 node
    // evaluation; the wrapped value disagrees with the range-check chain and
    // must be rejected rather than silently accepted.
    let circuit = balance_transfer([50, 20], [i64::MIN]);
    assert!(mock_check(&circuit).is_err(), "wrapping witness must fail");
    assert_eq!(
        checked(&ProveBalanceTransfer.build(), vec![50, 20], vec![i64::MIN]),
        WitnessError::Violated { index: 0 }
    );
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
