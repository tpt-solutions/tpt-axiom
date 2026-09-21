# Examples

Runnable examples for `tpt-axiom`, organized under each crate's own
`examples/` directory (standard Cargo convention).

- [`kalman_filter`](../crates/tpt-axiom/examples/kalman_filter.rs) — a 1-D
  Kalman filter built entirely from ordinary `+`/`*` arithmetic on
  `Fuzzy<f64>`, demonstrating automatic uncertainty propagation. This is the
  Phase 1 milestone example from `todo.md`.

Run an example with:

```sh
cargo run -p tpt-axiom --example kalman_filter
```

More examples (Monte Carlo simulation, sensor fusion, ZK balance-transfer,
ZK backend comparison) land here as later phases are implemented — see
[todo.md](../todo.md) Phase 5.
