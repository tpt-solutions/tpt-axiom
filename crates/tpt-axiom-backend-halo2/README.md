# tpt-axiom-backend-halo2

The [halo2](https://crates.io/crates/halo2_proofs) (IPA/Pallas) adapter for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

Implements `tpt_axiom_zk::ZkBackend` for the `halo2_proofs` proving stack: it
compiles the backend-agnostic `tpt_axiom_ir::ConstraintSystem` produced by
`#[zk_provable]` into a PLONKish circuit, runs real key generation (IPA
structured parameters over the Vesta curve), and produces proofs that verify
independently of the proving process.

## How the IR is lowered

* Every IR expression node gets one row; its value lives in an advice column,
  with operands copy-constrained alongside it and one selector-gated gate
  enforcing `+`, `-`, unary `-`, or `*`.
* Equality and zero constraints become copy constraints; constants are pinned
  with `assign_advice_from_constant`.
* Soundness over the field comes from bit-decomposition range checks: every
  named input is proven to be a **signed** `range_bits`-bit integer (default
  64, configurable via the `params` byte string) and every `NonNegative`
  constraint is proven to lie in `[0, 2^range_bits)`. Witnesses that overflow
  `i64` arithmetic are rejected at prove time by the IR-level witness check
  (`tpt_axiom_zk::witness`).

## Verifying-key portability

halo2 0.3 does not expose verifying-key serialization, so a verifier rebuilds
the key deterministically from the circuit IR and the same `k` (halo2 key
generation consumes no randomness). See `examples/balance_transfer_prove.rs`
and `examples/balance_transfer_verify.rs` for a prove/verify pair that runs
in separate processes:

```text
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_prove -- target/demo-proof
cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_verify -- target/demo-proof
```

## Benchmarks

`cargo bench -p tpt-axiom-backend-halo2` reports compile / keygen / prove /
verify for the shared `prove_balance_transfer` circuit (512 rows, `k = 9`),
with scenario names matching `tpt-axiom-backend-arkworks` for direct
comparison.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
