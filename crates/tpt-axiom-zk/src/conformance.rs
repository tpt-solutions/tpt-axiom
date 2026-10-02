//! Shared conformance drivers for [`ZkBackend`] implementations.
//!
//! Every backend adapter's test suite runs the same example circuits through
//! these generic functions and asserts identical prove/verify verdicts, so
//! backends stay semantically interchangeable:
//!
//! * `prove_balance_transfer` - a public transfer check with one secret
//!   witness (`NonNegative` + `Equal` constraints);
//! * `weighted_sum` - a public output (`return`) computed from a secret
//!   multiplier;
//! * `bounds_check` - three `NonNegative` comparisons over signed inputs,
//!   including negative witnesses;
//! * `not_equal` - the free-witness gadget the macro emits for `assert!(a !=
//!   b)`, exercising the solved-selector tail convention.
//!
//! Each driver covers the happy path plus the rejection paths: tampered
//! public inputs must fail verification, and violating secret witnesses must
//! fail proving. Backend errors are only required to implement `core::fmt::Display`,
//! so failures are reported through their display form.

use alloc::vec::Vec;

use tpt_axiom_ir::{ConstraintSystem, ConstraintSystemBuilder, Expr, Scalar};

use crate::ZkBackend;

/// The IR for `prove_balance_transfer`:
/// `assert!(sender >= amount)`, `new_receiver == receiver + amount`.
#[must_use]
pub fn balance_transfer_ir() -> ConstraintSystem {
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

/// The IR for `weighted_sum`: returns the public output `a*k + b` where `k`
/// is secret.
#[must_use]
pub fn weighted_sum_ir() -> ConstraintSystem {
    let mut b = ConstraintSystemBuilder::new("weighted_sum");
    let a = b.public_input("a");
    let b_in = b.public_input("b");
    let k = b.secret_input("k");
    let scaled = b.mul(a, k);
    let total = b.add(scaled, b_in);
    let out = b.output("return");
    b.constrain_eq(out, total);
    b.build()
}

/// The IR for `bounds_check`: `lo <= x <= hi` and `x < hi + 1`, all signed.
#[must_use]
pub fn bounds_check_ir() -> ConstraintSystem {
    let mut b = ConstraintSystemBuilder::new("bounds_check");
    let x = b.secret_input("x");
    let lo = b.public_input("lo");
    let hi = b.public_input("hi");
    let ge = b.sub(x, lo);
    b.constrain_non_negative(ge);
    let le = b.sub(hi, x);
    b.constrain_non_negative(le);
    let one = b.constant(1);
    let hi_plus = b.add(hi, one);
    let lt = b.sub(hi_plus, x);
    let strict = b.sub(lt, one);
    b.constrain_non_negative(strict);
    b.build()
}

/// The IR for `not_equal`: exactly what `#[zk_provable]` emits for
/// `assert!(l != r)` — a free boolean selector `s` with a booleanity
/// constraint and the two gated range checks `s = 1 ⇒ l ≥ r + 1`,
/// `s = 0 ⇒ l ≤ r − 1`. Exercises the free-witness tail convention end to
/// end: the aux value rides the secret slice after the named secrets.
#[must_use]
pub fn not_equal_ir() -> ConstraintSystem {
    let mut b = ConstraintSystemBuilder::new("not_equal");
    let l = b.public_input("l");
    let r = b.secret_input("r");
    let d = b.sub(l, r);
    let s = b.free_bool("__axiom_free0");
    let sd = b.mul(s, d);
    let ss = b.mul(s, s);
    b.constrain_eq(ss, s); // booleanity
    let arm1 = b.sub(sd, s); // s = 1 ⇒ d ≥ 1
    b.constrain_non_negative(arm1);
    let one = b.constant(1);
    let d1 = b.add(d, one);
    let sd1 = b.mul(s, d1);
    let arm2 = b.sub(sd1, d1); // s = 0 ⇒ d ≤ −1
    b.constrain_non_negative(arm2);
    b.build()
}

fn fail(context: &str, stage: &str, error: impl core::fmt::Display) -> ! {
    panic!("conformance [{context}]: {stage} failed unexpectedly: {error}")
}

/// Runs the full `prove_balance_transfer` conformance scenario on `backend`.
///
/// # Panics
/// Panics (with a `[context]` label) on any unexpected backend verdict.
pub fn run_balance_transfer<B: ZkBackend>(backend: &B) {
    let ir = balance_transfer_ir();
    let circuit = match backend.compile(&ir) {
        Ok(c) => c,
        Err(e) => fail("balance", "compile", e),
    };
    let (pk, vk) = match backend.generate_keys(&ir, &[]) {
        Ok(keys) => keys,
        Err(e) => fail("balance", "keygen", e),
    };

    // Happy path: transfer 30 from a balance of 50.
    let proof = match backend.prove(&circuit, &pk, &[50, 20], &[30]) {
        Ok(p) => p,
        Err(e) => fail("balance ok", "prove", e),
    };
    assert_verify(backend, &vk, &[50, 20], &proof, true, "balance ok");
    // The same proof bound to different public inputs must not verify.
    assert_verify(backend, &vk, &[50, 21], &proof, false, "balance tampered");

    // Overdraft: amount > sender violates the NonNegative constraint.
    assert_prove_rejected(
        backend,
        &circuit,
        &pk,
        &[50, 20],
        &[100],
        "balance overdraft",
    );
}

/// Runs the full `weighted_sum` conformance scenario on `backend`.
///
/// # Panics
/// Panics (with a `[context]` label) on any unexpected backend verdict.
pub fn run_weighted_sum<B: ZkBackend>(backend: &B) {
    let ir = weighted_sum_ir();
    let circuit = match backend.compile(&ir) {
        Ok(c) => c,
        Err(e) => fail("sum", "compile", e),
    };
    let (pk, vk) = match backend.generate_keys(&ir, &[]) {
        Ok(keys) => keys,
        Err(e) => fail("sum", "keygen", e),
    };

    // Happy path: 2*4 + 3 = 11.
    let proof = match backend.prove(&circuit, &pk, &[2, 3, 11], &[4]) {
        Ok(p) => p,
        Err(e) => fail("sum ok", "prove", e),
    };
    assert_verify(backend, &vk, &[2, 3, 11], &proof, true, "sum ok");
    // Wrong claimed output must not verify.
    assert_verify(backend, &vk, &[2, 3, 12], &proof, false, "sum wrong output");
}

/// Runs the full `bounds_check` conformance scenario on `backend`, including
/// negative signed witnesses.
///
/// # Panics
/// Panics (with a `[context]` label) on any unexpected backend verdict.
pub fn run_bounds_check<B: ZkBackend>(backend: &B) {
    let ir = bounds_check_ir();
    let circuit = match backend.compile(&ir) {
        Ok(c) => c,
        Err(e) => fail("bounds", "compile", e),
    };
    let (pk, vk) = match backend.generate_keys(&ir, &[]) {
        Ok(keys) => keys,
        Err(e) => fail("bounds", "keygen", e),
    };

    // Happy paths: inside the range, positive and negative.
    let pos = match backend.prove(&circuit, &pk, &[0, 10], &[5]) {
        Ok(p) => p,
        Err(e) => fail("bounds pos", "prove", e),
    };
    assert_verify(backend, &vk, &[0, 10], &pos, true, "bounds pos");

    let neg = match backend.prove(&circuit, &pk, &[-10, 10], &[-5]) {
        Ok(p) => p,
        Err(e) => fail("bounds neg", "prove", e),
    };
    assert_verify(backend, &vk, &[-10, 10], &neg, true, "bounds neg");
    // Negative publics re-bound to non-negative ones must not verify.
    assert_verify(backend, &vk, &[0, 10], &neg, false, "bounds tampered");

    // Out of range on both ends must fail proving.
    assert_prove_rejected(backend, &circuit, &pk, &[0, 10], &[11], "bounds above");
    assert_prove_rejected(backend, &circuit, &pk, &[-10, 10], &[-11], "bounds below");
}

/// Runs the full `not_equal` (free-witness gadget) conformance scenario on
/// `backend`.
///
/// The caller supplies the solved selector bit as the secret-slice tail (in
/// production the [`driver`](crate::driver) solves it); a positive and a
/// negative difference exercise both arms of the disjunction.
///
/// # Panics
/// Panics (with a `[context]` label) on any unexpected backend verdict.
pub fn run_not_equal<B: ZkBackend>(backend: &B) {
    let ir = not_equal_ir();
    let circuit = match backend.compile(&ir) {
        Ok(c) => c,
        Err(e) => fail("ne", "compile", e),
    };
    let (pk, vk) = match backend.generate_keys(&ir, &[]) {
        Ok(keys) => keys,
        Err(e) => fail("ne", "keygen", e),
    };

    // Happy path, positive arm: 10 ≠ 3 with selector s = 1.
    let proof = match backend.prove(&circuit, &pk, &[10], &[3, 1]) {
        Ok(p) => p,
        Err(e) => fail("ne pos", "prove", e),
    };
    assert_verify(backend, &vk, &[10], &proof, true, "ne pos");
    // Negative arm: −5 < 9 with selector s = 0.
    let neg = match backend.prove(&circuit, &pk, &[-5], &[9, 0]) {
        Ok(p) => p,
        Err(e) => fail("ne neg", "prove", e),
    };
    assert_verify(backend, &vk, &[-5], &neg, true, "ne neg");
    // A proof of −5 ≠ 9 says nothing about other publics.
    assert_verify(backend, &vk, &[9], &neg, false, "ne tampered");

    // l == r: neither selector value satisfies the range checks, in either
    // arm — and a wrong selector for a true difference is rejected too.
    assert_prove_rejected(backend, &circuit, &pk, &[7], &[7, 0], "ne equal s0");
    assert_prove_rejected(backend, &circuit, &pk, &[7], &[7, 1], "ne equal s1");
    assert_prove_rejected(backend, &circuit, &pk, &[10], &[3, 0], "ne wrong selector");
    // A non-binary selector violates the variable's declared 1-bit range.
    assert_prove_rejected(backend, &circuit, &pk, &[10], &[3, 2], "ne non-binary");
}

/// Runs every conformance scenario on `backend`.
///
/// # Panics
/// Panics on the first unexpected backend verdict, labeled by scenario.
pub fn run_all<B: ZkBackend>(backend: &B) {
    run_balance_transfer(backend);
    run_weighted_sum(backend);
    run_bounds_check(backend);
    run_not_equal(backend);
}

fn assert_verify<B: ZkBackend>(
    backend: &B,
    vk: &B::VerifyingKey,
    public: &[Scalar],
    proof: &B::Proof,
    expected: bool,
    context: &str,
) {
    match backend.verify(vk, public, proof) {
        Ok(verdict) => assert_eq!(
            verdict, expected,
            "conformance [{context}]: verification verdict mismatch"
        ),
        Err(e) => fail(context, "verify", e),
    }
}

/// # Panics
/// Panics if the backend accepts the false witness.
fn assert_prove_rejected<B: ZkBackend>(
    backend: &B,
    circuit: &B::Circuit,
    pk: &B::ProvingKey,
    public: &[Scalar],
    secret: &[Scalar],
    context: &str,
) {
    assert!(
        backend.prove(circuit, pk, public, secret).is_err(),
        "conformance [{context}]: proving a false witness must fail"
    );
}

/// Asserts an IR's public/secret variable classification matches expectations
/// (used by conformance tests to catch classification drift between macro
/// output and hand-built IRs).
#[allow(clippy::many_single_char_names)] // p/s over the two id lists
/// # Panics
/// Panics when the declared and actual variable classifications differ.
#[allow(clippy::option_if_let_else)] // the enum match reads clearest
pub fn assert_variable_shape(ir: &ConstraintSystem, public: &[&str], secret: &[&str]) {
    let names = |ids: &[usize]| -> Vec<&str> {
        ids.iter()
            .filter_map(|&id| match &ir.exprs[id] {
                Expr::Var(v) => ir.variables.get(*v).map(|info| info.name.as_str()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(names(&ir.public_inputs), public, "public variable drift");
    assert_eq!(names(&ir.secret_inputs), secret, "secret variable drift");
}
