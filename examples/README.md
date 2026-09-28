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

Run an example with:

```sh
cargo run -p tpt-axiom --example kalman_filter
cargo run -p tpt-axiom-backend-halo2 --example backend_comparison
```
