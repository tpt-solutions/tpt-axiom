# tpt-axiom-backend-halo2

The [halo2](https://github.com/zcash/halo2) adapter for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

## Status: Phase 4 scaffolding

This crate is intentionally an empty shell until the backend work lands. It
will implement `tpt_axiom_zk::ZkBackend` for halo2, generating real proving
and verifying keys at build time and producing proofs that verify
independently of the proving process.

The adapter receives the backend-agnostic `tpt_axiom_ir::ConstraintSystem`
produced by `#[zk_provable]` and will compile it into halo2's constraint form
(PLONKish gates); the reference R1CS lowering in `tpt-axiom-ir` defines the
semantics it must agree with.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
