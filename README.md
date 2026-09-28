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

Phases 1–4 are implemented and tested:

- **Phase 1** — `Fuzzy<T>` / `Distribution<T>` probabilistic types with
  error-propagating arithmetic (`tpt-axiom-core`).
- **Phase 2** — the `#[zk_provable]` macro lowering annotated Rust functions
  to a backend-agnostic arithmetic IR (`tpt-axiom-macros`, `tpt-axiom-ir`).
- **Phase 3** — `tpt-axiom-verify`: polynomial-identity checking that catches
  circuit mismatches and wrong variance formulas (`cargo test -p tpt-axiom-verify`).
- **Phase 4** — real ZK backends: halo2 (IPA/Vesta) and arkworks
  (Groth16/BLS12-381) produce and independently verify real proofs; SP1 is
  contract-scaffolded pending its Linux/macOS-only toolchain.

Remaining: expanded docs/examples polish (Phase 5) and the probabilistic
intelligence foundation — tracked in [todo.md](todo.md).

## Getting started

### 1. Uncertainty-propagating arithmetic

Add the umbrella crate to your `Cargo.toml`:

```toml
[dependencies]
tpt-axiom = { path = "crates/tpt-axiom" } # or a crates.io version, once published
```

Probabilistic types propagate uncertainty automatically through ordinary
arithmetic — every `+`, `-`, `*`, `/` applies the matching error-propagation
formula:

```rust
use tpt_axiom::prelude::*;

// Sensor readings with inherent uncertainty (mean, variance).
let pos_reading: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
let vel_reading: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
let dt = 1.0;

// Ordinary `+`/`*` — the propagated variance comes along for free.
let next_pos = pos_reading + (vel_reading * dt);

println!("{:.3} ± {:.3}", next_pos.mean(), next_pos.standard_deviation());
// 12.500 ± 0.742
```

Statistical helpers ride along: `standard_deviation()`,
`confidence_interval(level)` (a `(low, high)` tuple), and `z_score(value)`.
`Distribution<T>` covers the general (currently Gaussian/constant) case.

Run the worked examples:

```sh
cargo run -p tpt-axiom --example kalman_filter    # 1-D Kalman filter from plain +/*
cargo run -p tpt-axiom --example monte_carlo      # simulation vs analytic propagation
cargo run -p tpt-axiom --example sensor_fusion    # minimum-variance sensor fusion
```

### 2. Zero-knowledge circuits from ordinary functions

Annotate a plain Rust function with `#[zk_provable]`, classify parameters as
`#[public]` (default) or `#[secret]`, and state the constraints with
`assert!`/`assert_eq!`. The macro keeps the original function as runnable
Rust *and* lowers the constraints to a backend-agnostic circuit:

```rust
use tpt_axiom::prelude::*;

#[zk_provable(backend = "halo2")]
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

// Still plain Rust — run it directly:
prove_balance_transfer(50, 20, 30);

// …and a circuit definition lowering to the IR:
let ir = ProveBalanceTransfer.build();
assert_eq!(ir.num_public(), 2); // sender, receiver
assert_eq!(ir.num_secret(), 1); // amount
```

Supported body syntax is straight-line integer arithmetic (`+`, `-`, `*`,
literals, `let` bindings) plus `assert!` comparisons (`>=`, `<=`, `>`, `<`,
`==`); everything else gets a clear compile error.

### 3. Real proofs with a backend adapter

Pick a backend crate and drive the `ZkBackend` trait — compile, generate
keys, prove, and verify independently:

```toml
[dependencies]
tpt-axiom-backend-halo2 = { path = "crates/tpt-axiom-backend-halo2" }
```

```rust
use tpt_axiom::prelude::*;
use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_zk::ZkBackend;

# fn demo() -> Result<(), Box<dyn std::error::Error>> {
let backend = Halo2Backend;
let ir = ProveBalanceTransfer.build();
let circuit = backend.compile(&ir)?;
let (pk, vk) = backend.generate_keys(&ir, &[])?;
let proof = backend.prove(&circuit, &pk, &[50, 20], &[30])?;
assert!(backend.verify(&vk, &[50, 20], &proof)?);
# Ok(())
# }
```

The same `prove`/`verify` calls work on
`tpt_axiom_backend_arkworks::ArkworksBackend` (Groth16/BLS12-381). Circuit
semantics are signed 64-bit integers: inputs are range-checked, and
violating witnesses are rejected at prove time with a constraint index.
Witness a full run in separate processes:

```sh
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_prove  -- target/demo-proof
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_verify -- target/demo-proof
cargo run -p tpt-axiom-backend-halo2 --example backend_comparison      # both backends, side by side
```

### Choosing a backend

`cargo bench -p tpt-axiom-backend-halo2 -p tpt-axiom-backend-arkworks` is the
authoritative comparison; reference numbers for `prove_balance_transfer`
(512 halo2 rows / 264 arkworks R1CS constraints) on a dev workstation:

| Backend            | Setup (keygen) | Prove   | Verify  |
| ------------------ | -------------- | ------- | ------- |
| halo2 (IPA/Vesta)  | ≈ 17 ms     | ≈ 8 ms  | ≈ 3 ms  |
| arkworks (Groth16) | ≈ 414 ms    | ≈ 338 ms | ≈ 10 ms |

halo2 wins on proving speed and has no trusted setup; arkworks Groth16
produces the smallest verifiable objects with canonical key serialization.

## Workspace layout

| Crate                        | Purpose                                                            |
| ---------------------------- | ------------------------------------------------------------------- |
| `tpt-axiom-core`             | `Fuzzy<T>` / `Distribution<T>` probabilistic types (Phase 1)        |
| `tpt-axiom-ir`                | Shared backend-agnostic arithmetic IR (Phase 2)                     |
| `tpt-axiom-macros`           | `#[zk_provable]` proc-macro (Phase 2)                               |
| `tpt-axiom-zk`                | `ZkBackend` trait + circuit/proving-key/verifying-key abstractions  |
| `tpt-axiom-verify`            | Polynomial-identity circuit/variance-formula verification (Phase 3) |
| `tpt-axiom-backend-halo2`     | halo2 backend adapter (Phase 4)                                     |
| `tpt-axiom-backend-arkworks`  | arkworks backend adapter (Phase 4)                                  |
| `tpt-axiom-backend-sp1`       | sp1 backend adapter (Phase 4)                                       |
| `tpt-axiom-cli`               | Build-time driver (key generation, verification)                    |
| `tpt-axiom`                   | Umbrella crate re-exporting the public `prelude`                    |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
