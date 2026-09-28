# tpt-axiom-zk

Backend-agnostic zero-knowledge abstractions for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) — `no_std`.

Two traits sit at the seam between the Rust source and any concrete proving
system:

- [`CircuitDefinition`] — implemented by the `#[zk_provable]` macro (from
  `tpt-axiom-macros`) for every annotated function. Its `build` method lowers
  the function's constraints into a `tpt_axiom_ir::ConstraintSystem`.
- [`ZkBackend`] — the trait adapter crates (`tpt-axiom-backend-halo2`,
  `tpt-axiom-backend-arkworks`, `tpt-axiom-backend-sp1`) implement to compile
  an IR circuit, generate keys, prove, and verify.

```rust,ignore
fn compile_and_prove<B: ZkBackend>(backend: &B, circuit: &impl CircuitDefinition) {
    let ir = circuit.build();
    let compiled = backend.compile(&ir).unwrap();
    let (pk, vk) = backend.generate_keys(&ir, &[]).unwrap();
    // ...
}
```

## Status

No real backend is wired up yet (Phase 4) — this crate defines the stable
contract that `tpt-axiom-backend-halo2` / `-arkworks` / `-sp1` will implement.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
