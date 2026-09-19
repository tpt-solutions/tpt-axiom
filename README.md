# tpt-axiom

**Probabilistic Logic and Native Zero-Knowledge State for Rust.**

`tpt-axiom` brings advanced mathematical reasoning natively into Rust. It bridges
deterministic systems programming and the stochastic, provable mathematics
required by autonomous systems, quantitative finance, and Web3 — letting you
write probabilistic algorithms and zero-knowledge circuits in standard Rust
syntax, with the compiler handling the heavy mathematical lifting.

See [spec.txt](spec.txt) for the full design and [ARCHITECTURE.md](ARCHITECTURE.md)
for how the pieces fit together. Development is tracked phase-by-phase in
[todo.md](todo.md).

License: dual **MIT OR Apache-2.0**. Author: **TPT Solutions**.

## Status

Phase 1 (probabilistic & uncertainty types) is implemented in `axiom-core`.
Later phases (`#[zk_provable]`, `tpt-telos` verification, ZK backend adapters)
are scaffolded but not yet implemented — see [todo.md](todo.md) for the
current phase.

## Quickstart

Add the umbrella crate to your `Cargo.toml`:

```toml
[dependencies]
tpt-axiom = { path = "crates/tpt-axiom" } # or a crates.io version, once published
```

Probabilistic types propagate uncertainty automatically through ordinary
arithmetic:

```rust
use tpt_axiom::prelude::*;

// Sensor readings with inherent uncertainty (mean, variance).
let pos_reading: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
let vel_reading: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
let dt = 1.0;

// Ordinary `+`/`*` — the propagated variance comes along for free.
let next_pos = pos_reading + (vel_reading * dt);

println!("{:?} ± {:.3}", next_pos.mean(), next_pos.std_dev());
```

Run the Kalman-filter example:

```sh
cargo run -p tpt-axiom --example kalman_filter
```

See [examples/README.md](examples/README.md) for more.

## Workspace layout

| Crate                      | Purpose                                                              |
| --------------------------- | --------------------------------------------------------------------- |
| `axiom-core`                | `Fuzzy<T>` / `Distribution<T>` probabilistic types (Phase 1)          |
| `axiom-ir`                  | Shared backend-agnostic arithmetic IR (Phase 2)                       |
| `axiom-macros`              | `#[zk_provable]` proc-macro (Phase 2)                                 |
| `axiom-zk`                  | `ZkBackend` trait + circuit/proving-key/verifying-key abstractions    |
| `axiom-backend-halo2`       | halo2 backend adapter (Phase 4)                                       |
| `axiom-backend-arkworks`    | arkworks backend adapter (Phase 4)                                    |
| `axiom-backend-sp1`         | sp1 backend adapter (Phase 4)                                         |
| `axiom-cli`                 | Build-time driver (key generation, verification)                     |
| `tpt-axiom`                 | Umbrella crate re-exporting the public `prelude`                      |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
