# tpt-axiom-backend-sp1

The [SP1](https://github.com/succinctlabs/sp1) adapter for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

## Status: Phase 4 scaffolding

This crate is intentionally an empty shell until the backend work lands. It
will implement `tpt_axiom_zk::ZkBackend` for SP1, compiling the
backend-agnostic `tpt_axiom_ir::ConstraintSystem` into an SP1 program and
generating real proofs.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
