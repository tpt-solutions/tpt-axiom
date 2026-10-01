# Examples

Runnable examples for `tpt-axiom`, organized under each crate's own
`examples/` directory (standard Cargo convention).

## Probabilistic types (`tpt-axiom`)

- [`kalman_filter`](../crates/tpt-axiom/examples/kalman_filter.rs) — a 1-D
  Kalman filter built entirely from ordinary `+`/`*` arithmetic on
  `Fuzzy<f64>`, demonstrating automatic uncertainty propagation.
- [`monte_carlo`](../crates/tpt-axiom/examples/monte_carlo.rs) — a 500k-sample
  Monte Carlo simulation cross-checked against the analytic error-propagation
  formulas `Fuzzy<f64>` applies.
- [`sensor_fusion`](../crates/tpt-axiom/examples/sensor_fusion.rs) — three
  noisy sensors fused with the minimum-variance (Kalman-style) combination,
  showing the fused estimate is sharper than every input.

## Zero-knowledge circuits (`tpt-axiom-backend-halo2`)

- [`balance_transfer_prove`](../crates/tpt-axiom-backend-halo2/examples/balance_transfer_prove.rs)
  / [`balance_transfer_verify`](../crates/tpt-axiom-backend-halo2/examples/balance_transfer_verify.rs)
  — the Phase 2/4 milestone circuit proven with real halo2 IPA proofs and
  verified by a separate process that runs no proving code.
- [`backend_comparison`](../crates/tpt-axiom-backend-halo2/examples/backend_comparison.rs)
  — the same circuit through halo2 and arkworks side by side (one-glance
  timings; `cargo bench` is the statistically sound version).
- [`age_proof`](../crates/tpt-axiom-backend-halo2/examples/age_proof.rs) —
  prove you are old enough without revealing your age; includes an
  adversarial underage witness being rejected at prove time.
- [`sensor_fusion_claim`](../crates/tpt-axiom-backend-halo2/examples/sensor_fusion_claim.rs)
  — verifiable sensor fusion: publish only the fused estimate and prove it
  is the minimum-variance combination of two secret readings (fixed-point
  `Fuzzy::fuse` as ZK constraints; three forgery paths rejected).
- [`ml_decision_abstention`](../crates/tpt-axiom/examples/ml_decision_abstention.rs)
  — an ML classifier whose low-confidence outputs abstain with a reason
  instead of guessing: logits → normalized distribution → explicit
  commit/review/reject policy.
- [`ab_test`](../crates/tpt-axiom/examples/ab_test.rs) — an A/B test decided
  on Beta posteriors: conjugate updates, credible intervals, `P(B > A)` by
  quadrature, and an explicit ship bar that thin evidence honestly fails.
- [`credit_score_gate`](../crates/tpt-axiom-backend-halo2/examples/credit_score_gate.rs)
  — prove an applicant clears a lending threshold without revealing the
  score (public scorecard, secret data, near-miss and tampered-scorecard
  forgeries).
- [`custom_backend`](../crates/tpt-axiom-zk/examples/custom_backend.rs) — a
  `ZkBackend` adapter skeleton for backend authors: the full trait contract
  over the reference R1CS lowering, with zero cryptography and every step
  annotated.

Run an example with:

```sh
cargo run -p tpt-axiom --example kalman_filter
cargo run -p tpt-axiom-backend-halo2 --example backend_comparison
```
