# tpt-axiom

Probabilistic logic and native zero-knowledge state for Rust — the umbrella
crate re-exporting the whole `tpt-axiom` ecosystem behind a single
`use tpt_axiom::prelude::*;`.

This crate has no logic of its own: it re-exports `tpt-axiom-core`,
`tpt-axiom-ir`, `tpt-axiom-macros`, and `tpt-axiom-zk`, and defines the
`prelude` module. Depend on this crate unless you specifically need one
sub-crate in isolation (e.g. implementing a new ZK backend adapter).

## Probabilistic types

```rust
use tpt_axiom::prelude::*;

let pos: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
let vel: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
let dt = 0.2_f64;

let next_pos = pos + vel * dt; // uncertainty propagates automatically
assert!((next_pos.mean() - 10.9).abs() < 1e-9);
```

## Zero-knowledge state

```rust
use tpt_axiom::prelude::*;

#[zk_provable(backend = "halo2")]
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
}

let circuit = ProveBalanceTransfer;
let ir = circuit.build();
assert_eq!(ir.num_public(), 2);
```

See [`examples/kalman_filter.rs`](examples/kalman_filter.rs) for a worked
sensor-fusion example, and the workspace [README](../../README.md) /
[ARCHITECTURE.md](../../ARCHITECTURE.md) for how the sub-crates fit together.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
