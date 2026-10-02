# Cookbook

End-to-end recipes for the common shapes. Every snippet uses the APIs the
rest of the book introduces; the full runnable versions live in
`crates/tpt-axiom-examples/examples/` (run any of them with
`cargo run -p tpt-axiom-examples --example <name>`).

## Prove a value clears a bar without revealing it

The bread-and-butter range claim: the prover holds a secret number, the
verifier learns only that it clears a public threshold.

```rust
use tpt_axiom::prelude::*;

#[zk_provable(backend = "halo2")]
fn prove_score_at_least(
    #[public] cutoff: i64,
    #[secret] score: i64,
) {
    assert!(score >= cutoff);
}
```

The comparison lowers to `score − cutoff ≥ 0`, and the `NonNegative`
constraint becomes a bit-decomposition range check in the backend — so the
proof fails unless `score` really is at least `cutoff`, while the verifier
sees only `cutoff`. Declared types matter: a `u32` parameter is proven in
`[0, 2^32)`, a signed `i64` in `[-2^63, 2^63)`.

Run it: `cargo run -p tpt-axiom-examples --example age_proof`.

## Prove two values differ (`!=`)

`assert!(a != b)` lowers to the inequality gadget — a free boolean selector
with two gated range checks. A satisfying assignment exists exactly when the
values really differ, so a false equality is refused *before any proving
work* (the driver solves the selector, finds none, and errors out):

```rust
#[zk_provable(backend = "halo2")]
fn prove_distinct_bid(#[public] reserve: i64, #[secret] bid: i64) {
    assert!(bid != reserve);
    assert!(bid >= 0);
}
```

The selector is never part of the witness you supply:
`ProveDistinctBidInputs::new(reserve, bid).named()` is the whole prover-side
story.

## Branch on a secret condition (`if`)

Both arms lower; each arm's constraints are gated by a selector pinned to
the condition's truth, so exactly one arm's claims are enforced and the
verifier cannot tell which:

```rust
#[zk_provable(backend = "arkworks")]
fn prove_risk_band(
    #[public] low: i64,
    #[public] high: i64,
    #[secret] exposure: i64,
    #[secret] collateral: i64,
) {
    if exposure >= high {
        assert!(collateral >= exposure); // fully collateralized when large
    } else {
        assert!(exposure >= low); // within band when small
    }
}
```

All six comparison operators work as conditions (`==` and `!=` included),
branches may bind their own `let`s, and `if`/`else` *expressions* multiplex
values: `let tier = if volume >= 100 { 2 } else { 1 };` produces a public
output computed from secret data.

## Unrolled rounds with an accumulator (`for`)

A `for` loop over a *literal* range unrolls; a `let mut` accumulator
rebinds as a fresh value each step. Together they cover the sum/product
patterns:

```rust
#[zk_provable(backend = "arkworks")]
fn prove_weekly_total(#[secret] daily: i64, #[public] total: i64) {
    let mut acc = 0;
    for i in 0..7 {
        acc += daily; // 7 · daily, one constraint per step
    }
    assert_eq!(acc, total);
}
```

The counter is usable in the body (`acc += i * daily;` weights each step),
unrolling is capped at 1 024 iterations, and the counter never leaks past
the loop.

## Prove a derived statistic over secret data

Anything expressible as integer arithmetic can be proven: a mean at a fixed
scale, a weighted sum, a fusion. The verifiable sensor-fusion claim is the
worked example — the circuit proves the *published* fused mean/variance is
the minimum-variance fusion of two secret readings, in exact integer
arithmetic, and the example then forges three different ways to show the
forgeries are rejected with constraint indices:

```sh
cargo run -p tpt-axiom-examples --example sensor_fusion_claim
```

The pattern to copy: publish the derived value as a public input, constrain
it to the arithmetic the claim describes, and keep the raw inputs secret.

## Ship a proof to a verifier

`ProofEnvelope` is the wire format: backend name, circuit name, the IR-digest
binding, the public inputs, and the proof bytes. The digest binding means a
proof can never be re-verified against a different (say, weakened) circuit
definition — the rebuild checks the commitment first and answers a clean
`false`:

```rust,ignore
let claim = prove_named(&backend, &ProveScoreAtLeast, &pk, &witness)?;
let envelope = claim.to_envelope(&backend).expect("halo2 proofs are bytes");
// …send `envelope` as JSON to the verifier…
let rebuilt = ProofClaim::from_envelope(&backend, &envelope).expect("matching backend");
assert!(verify_claim(&backend, &ProveScoreAtLeast, &vk, &rebuilt)?);
```

`examples/proof_service.rs` runs the whole loop over localhost TCP (JSON
witness in, envelope back, verdict on a tampered case), and
`examples/onchain_export.rs` emits a Solidity Groth16 verifier plus a forge
test for the same proof.

## Debug a prove that fails

The prove path is checked *before* proving, so failures are precise:

* **`NamedWitnessError::MissingInput` / `UnknownInput`** — a typo or a field
  from another circuit; the name-keyed API catches it before any arithmetic.
* **`WitnessError::Violated { index }`** — the named constraint is not
  satisfied by your witness. `cargo axiom inspect <circuit>` prints the
  public/secret layout slot by slot; the index points into the constraint
  list the IR prints.
* **`WitnessError::NonNegativeOutOfRange`** — the claim's gap is honest but
  wider than the 64-bit range-check chain (default `range_bits`); widen
  `KeygenOptions` or narrow the claim.
* **`FreeVariablesUnsatisfiable`** — for `!=`/`if` gadgets this means the
  *claim itself is false* (the equality holds, or neither branch is
  satisfiable); no selector assignment works.

`cargo axiom check` walks every registered circuit and re-validates the IRs,
which catches lowering drift at build time.
