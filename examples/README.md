# Examples

Runnable examples for `tpt-axiom`, organized under each crate's own
`examples/` directory (standard Cargo convention).

## Probabilistic types (`tpt-axiom`)

- [`kalman_filter`](../crates/tpt-axiom-examples/examples/kalman_filter.rs) — a 1-D
  Kalman filter built entirely from ordinary `+`/`*` arithmetic on
  `Fuzzy<f64>`, demonstrating automatic uncertainty propagation.
- [`monte_carlo`](../crates/tpt-axiom-examples/examples/monte_carlo.rs) — a 500k-sample
  Monte Carlo simulation cross-checked against the analytic error-propagation
  formulas `Fuzzy<f64>` applies.
- [`sensor_fusion`](../crates/tpt-axiom-examples/examples/sensor_fusion.rs) — three
  noisy sensors fused with the minimum-variance (Kalman-style) combination,
  showing the fused estimate is sharper than every input.

## Zero-knowledge circuits (`tpt-axiom-backend-halo2`)

- [`balance_transfer_prove`](../crates/tpt-axiom-examples/examples/balance_transfer_prove.rs)
  / [`balance_transfer_verify`](../crates/tpt-axiom-examples/examples/balance_transfer_verify.rs)
  — the Phase 2/4 milestone circuit proven with real halo2 IPA proofs and
  verified by a separate process that runs no proving code.
- [`backend_comparison`](../crates/tpt-axiom-examples/examples/backend_comparison.rs)
  — the same circuit through halo2 and arkworks side by side (one-glance
  timings; `cargo bench` is the statistically sound version).
- [`age_proof`](../crates/tpt-axiom-examples/examples/age_proof.rs) —
  prove you are old enough without revealing your age; includes an
  adversarial underage witness being rejected at prove time.
- [`sensor_fusion_claim`](../crates/tpt-axiom-examples/examples/sensor_fusion_claim.rs)
  — verifiable sensor fusion: publish only the fused estimate and prove it
  is the minimum-variance combination of two secret readings (fixed-point
  `Fuzzy::fuse` as ZK constraints; three forgery paths rejected).
- [`ml_decision_abstention`](../crates/tpt-axiom-examples/examples/ml_decision_abstention.rs)
  — an ML classifier whose low-confidence outputs abstain with a reason
  instead of guessing: logits → normalized distribution → explicit
  commit/review/reject policy.
- [`ab_test`](../crates/tpt-axiom-examples/examples/ab_test.rs) — an A/B test decided
  on Beta posteriors: conjugate updates, credible intervals, `P(B > A)` by
  quadrature, and an explicit ship bar that thin evidence honestly fails.
- [`credit_score_gate`](../crates/tpt-axiom-examples/examples/credit_score_gate.rs)
  — prove an applicant clears a lending threshold without revealing the
  score (public scorecard, secret data, near-miss and tampered-scorecard
  forgeries).
- [`custom_backend`](../crates/tpt-axiom-examples/examples/custom_backend.rs) — a
  `ZkBackend` adapter skeleton for backend authors: the full trait contract
  over the reference R1CS lowering, with zero cryptography and every step
  annotated.
- [`proof_service`](../crates/tpt-axiom-examples/examples/proof_service.rs)
  — separate prover and verifier services over localhost TCP: the secret
  witness stays in the prover process, a `ProofEnvelope` is the only thing
  that crosses the wire, and a tampered envelope is refused.

Run an example with:

```sh
cargo run -p tpt-axiom-examples --example kalman_filter
cargo run -p tpt-axiom-examples --example backend_comparison
```
