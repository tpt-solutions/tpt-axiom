# tpt-axiom

**Probabilistic Logic and Native Zero-Knowledge State for Rust.**

`tpt-axiom` brings advanced mathematical reasoning natively into Rust. It bridges
deterministic systems programming and the stochastic, provable mathematics
required by autonomous systems, quantitative finance, and Web3 — letting you
write probabilistic algorithms and zero-knowledge circuits in standard Rust
syntax, with the compiler handling the heavy mathematical lifting.

See [the design document](docs/design/spec.md) for the full spec and
[ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together. Development
is tracked phase-by-phase in [todo.md](todo.md).

License: dual **MIT OR Apache-2.0**. Author: **TPT Solutions**.

## Status

**This crate has not been independently audited.** The zero-knowledge
implementations are new, the trusted-setup story for the arkworks (Groth16)
backend is development-grade (see [SECURITY.md](SECURITY.md)), and the
probabilistic arithmetic assumes independent inputs. Treat the numbers this
crate produces as engineering-grade, not life-safety-grade.

Works today (implemented, tested, documented):

- `Fuzzy<T>` / `Distribution<T>` probabilistic types with error-propagating
  arithmetic, plus the `Probability`/`Confidence`/`Decision`/`Evidence`
  intelligence types for AI/decision workloads (`tpt-axiom-core`).
- The `#[zk_provable]` macro lowering annotated Rust functions to a
  backend-agnostic arithmetic IR (`tpt-axiom-macros`, `tpt-axiom-ir`).
- `tpt-axiom-verify`: polynomial-identity checking that catches circuit
  mismatches and wrong variance formulas.
- Real ZK backends: halo2 (IPA/Vesta) and arkworks (Groth16/BLS12-381) produce
  and independently verify real proofs.
- Vendor-neutral interop: Augur distribution conversions, the inference-sample
  boundary, and external-engine interfaces (`tpt-axiom-interop`).

Experimental / incomplete:

- SP1 zkVM backend — the `ZkBackend` contract is scaffolded; the actual
  IR-to-program lowering awaits SP1's Linux/macOS-only toolchain.
- `tpt-axiom-cli` — a minimal driver today; the full `cargo axiom`
  UX (inspect, check, bench) is on the roadmap.

Missing (see the roadmap in [todo.md](todo.md)): correlated uncertainty,
nonlinear transforms, more distribution families, WASM verifiers.

## Getting started

### 1. Uncertainty-propagating arithmetic

Add the umbrella crate to your `Cargo.toml`:

```toml
[dependencies]
tpt-axiom = "0.1" # or a path/git dependency while pre-release
```

Probabilistic types propagate uncertainty automatically through ordinary
arithmetic — every `+`, `-`, `*`, `/` applies the matching error-propagation
formula. `Fuzzy` stores a mean and a **variance** (the squared standard
error); use [`Fuzzy::from_std_dev`](crates/tpt-axiom-core/src/fuzzy.rs) when
you have the sigma form:

```rust
use tpt_axiom::prelude::*;

// Sensor readings with inherent uncertainty (mean, variance).
let pos_reading: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
let vel_reading: Fuzzy<f64> = Fuzzy::from_std_dev(2.0, 0.1_f64.sqrt());
let dt = 1.0;

// Ordinary `+`/`*` — the propagated variance comes along for free.
let next_pos = pos_reading + (vel_reading * dt);

println!("{:.3} ± {:.3}", next_pos.mean(), next_pos.standard_deviation());
// 12.500 ± 0.775   (sqrt(0.5 + 0.1) ≈ 0.7746 — the README's ± is a std-dev)
```

Statistical helpers ride along: `standard_deviation()`,
`confidence_interval(level)` (a `(low, high)` tuple), and `z_score(value)`.
`Distribution<T>` covers the general (currently Gaussian/constant) case.

Beyond `Fuzzy`, the intelligence types model decisions rather than
measurements: a validated `Probability` (chance of an outcome) is a different
type from `Confidence` (belief in a result), and `Decision<T>` makes
abstention a first-class outcome instead of an ad-hoc `None`:

```rust
use tpt_axiom::prelude::*;

let score = Score::new("approve", Confidence::new_or_panic(0.62));
let threshold = Confidence::new_or_panic(0.8);

// Explicit policy: commit above the threshold, abstain below — never a
// silent guess.
let decision = Decision::from_score(score, threshold);
assert_eq!(decision.committed(), None);
```

Run the worked examples:

```sh
cargo run -p tpt-axiom --example kalman_filter    # 1-D Kalman filter from plain +/*
cargo run -p tpt-axiom --example monte_carlo      # simulation vs analytic propagation
cargo run -p tpt-axiom --example sensor_fusion    # minimum-variance sensor fusion
```

### 2. Zero-knowledge circuits from ordinary functions

Annotate a plain Rust function with `#[zk_provable]`, classify parameters as
`#[public]` (default) or `#[secret]`, and state the constraints with
`assert!`/`assert_eq!` (conjunctions with `&&` allowed). The macro keeps the
original function as runnable Rust *and* lowers the constraints to a
backend-agnostic circuit:

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
`==`, and `&&` between them); everything else gets a clear compile error.

### 3. Real proofs with a backend adapter

Pick a backend feature on the umbrella crate (`tpt-axiom = { version = "0.1",
features = ["halo2"] }`) or depend on the adapter crate directly, then drive
the `ZkBackend` trait — compile, generate keys, prove, and verify
independently:

```rust,ignore
// tested in tpt-axiom-backend-halo2's integration suite; needs the halo2 feature
use tpt_axiom::prelude::*;

let backend = Halo2Backend;
let ir = ProveBalanceTransfer.build();
let circuit = backend.compile(&ir)?;
let (pk, vk) = backend.generate_keys(&ir, &[])?;
let proof = backend.prove(&circuit, &pk, &[50, 20], &[30])?;
assert!(backend.verify(&vk, &[50, 20], &proof)?);
```

The same `prove`/`verify` calls work on `ArkworksBackend` (Groth16/BLS12-381;
see its crate docs for the per-circuit trusted-setup caveats). Circuit
semantics are the declared integer types: inputs are range-checked against
their own width, intermediate expressions are statically bounded away from
field wraparound, and violating witnesses are rejected at prove time with a
constraint index. Witness a full run in separate processes:

```sh
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_prove  -- target/demo-proof
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_verify -- target/demo-proof
cargo run -p tpt-axiom-backend-halo2 --example age_proof              # prove age ≥ 18 without revealing it
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

halo2 wins on proving speed and has no per-circuit trusted setup; arkworks
Groth16 produces the smallest verifiable objects with canonical key
serialization (but needs a honest per-circuit setup ceremony).

## Workspace layout

| Crate                        | Purpose                                                              |
| ---------------------------- | -------------------------------------------------------------------- |
| `tpt-axiom-core`             | `Fuzzy<T>`, `Distribution<T>`, and the intelligence decision types   |
| `tpt-axiom-ir`               | Shared backend-agnostic arithmetic IR                                |
| `tpt-axiom-macros`           | `#[zk_provable]` proc-macro                                          |
| `tpt-axiom-zk`               | `ZkBackend` trait, witness checking, `ProofClaim`                    |
| `tpt-axiom-verify`           | Polynomial-identity circuit/variance-formula verification            |
| `tpt-axiom-backend-halo2`    | halo2 backend adapter (IPA/Vesta)                                    |
| `tpt-axiom-backend-arkworks` | arkworks backend adapter (Groth16/BLS12-381)                         |
| `tpt-axiom-backend-sp1`      | SP1 zkVM adapter contract (proving deferred)                         |
| `tpt-axiom-cli`              | Build-time driver (`axiom` binary)                                   |
| `tpt-axiom-interop`          | Augur/inference conversions + external-engine interfaces             |
| `tpt-axiom`                  | Umbrella crate: the `prelude`, feature-gated re-exports              |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the security policy in
[SECURITY.md](SECURITY.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
