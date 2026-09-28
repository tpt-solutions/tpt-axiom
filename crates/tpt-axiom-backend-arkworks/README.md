# tpt-axiom-backend-arkworks

The [arkworks](https://github.com/arkworks-rs) adapter for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

## Status: Phase 4 scaffolding

This crate is intentionally an empty shell until the backend work lands. It
will implement `tpt_axiom_zk::ZkBackend` for the arkworks proving stack,
converting the backend-agnostic `tpt_axiom_ir::ConstraintSystem` into
arkworks R1CS.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
