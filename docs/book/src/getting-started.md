# Getting started

Add the umbrella crate:

```toml
[dependencies]
tpt-axiom = { version = "0.1", features = ["halo2"] } # halo2 for real proofs
```

Then run the two halves of the library — uncertainty arithmetic and a real
proof:

```rust
use tpt_axiom::prelude::*;

// 1. Uncertainty arithmetic: readings carry (mean, variance).
let pos = Fuzzy::new(10.5, 0.5);
let vel = Fuzzy::from_std_dev(2.0, 0.1_f64.sqrt());
let next = pos + vel * 1.0;
println!("{:.3} ± {:.3}", next.mean(), next.standard_deviation());

// 2. A zero-knowledge circuit from a plain function.
#[zk_provable(backend = "halo2")]
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
}

let backend = Halo2Backend;
let ir = ProveBalanceTransfer.build();
let circuit = backend.compile(&ir).unwrap();
let (pk, vk) = backend.generate_keys(&ir, &[]).unwrap();
let proof = backend.prove(&circuit, &pk, &[50, 20], &[30]).unwrap();
assert!(backend.verify(&vk, &[50, 20], &proof).unwrap());
```

The function above still runs as plain Rust — `prove_balance_transfer(50,
20, 30)` — and *also* lowers to a backend-agnostic circuit. Prefer starting
from the examples in the repository (`examples/README.md`); each documents
its expected output and runs in CI. For a fresh project, `tpt-axiom-template`
generates one via `cargo generate`.
