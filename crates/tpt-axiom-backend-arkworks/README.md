# tpt-axiom-backend-arkworks

The [arkworks](https://arkworks.rs) (Groth16/BLS12-381) adapter for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

Implements `tpt_axiom_zk::ZkBackend` for the arkworks proving stack: it
lowers the backend-agnostic `tpt_axiom_ir::ConstraintSystem` produced by
`#[zk_provable]` into arkworks R1CS, runs Groth16 circuit-specific setup, and
produces proofs that verify independently of the proving process.

## How the IR is lowered

* Every IR expression node becomes an arkworks witness variable tied to its
  operands with `a·b = c` constraints (`Mul` is a genuine quadratic gate;
  `+`, `-`, unary `-` constrain against the constant one).
* Equality and zero constraints become linear gates.
* Soundness over the field comes from bit-decomposition range checks: every
  named input is proven to fit its **declared** integer type (`u8` to
  `[0, 2^8)`, `i64` to `[-2^63, 2^63)`, capped by the `range_bits` param,
  default 64) and every `NonNegative` constraint is proven to lie in
  `[0, 2^range_bits)`. Witnesses that violate the IR are rejected at prove
  time by `tpt_axiom_zk::witness`; arkworks' own `cs.is_satisfied()` assert is
  the second line of defense.

## Key material serialization

Unlike halo2 0.3, arkworks exposes canonical serialization for proving keys,
verifying keys, and proofs (`to_bytes` re-exports the compressed form), so
verifiers can consume shipped key material directly — see
`tests/proofs.rs::keys_and_proofs_roundtrip_through_bytes`.

## Benchmarks

`cargo bench -p tpt-axiom-backend-arkworks` reports compile / keygen / prove
/ verify for the shared `prove_balance_transfer` circuit (264 R1CS
constraints), with scenario names matching `tpt-axiom-backend-halo2` for
direct comparison.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
