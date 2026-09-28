# tpt-axiom-ir

The shared arithmetic intermediate representation (IR) for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) — `no_std`, and
backend-agnostic.

This is the format that the [`#[zk_provable]`](https://docs.rs/tpt-axiom-macros)
macro (from `tpt-axiom-macros`) lowers annotated Rust functions into. It sits
between the Rust source (application logic) and any concrete zero-knowledge
backend (`halo2`, `arkworks`, `sp1`).

## What's in it

- An expression DAG built from [`ConstraintSystemBuilder`] operations
  (constants, variables, `+`, `-`, `*`, unary `-`).
- A set of [`Constraint`]s asserting equality, zero, or non-negativity.
- A lowering to [`R1CS`] quadratic constraints, demonstrating the IR is
  expressive enough for the standard ZK constraint forms.

For now the scalar "field" is modelled as [`Scalar`] = `i64`; backend adapter
crates (`tpt-axiom-backend-halo2`, `tpt-axiom-backend-arkworks`,
`tpt-axiom-backend-sp1`) will translate IR expressions into their native
field during compilation (Phase 4).

## Who uses this crate

You generally don't depend on `tpt-axiom-ir` directly — it's consumed by
`tpt-axiom-macros` (which builds it from your annotated function) and
`tpt-axiom-zk` (whose `ZkBackend` trait consumes it). Depend on the
`tpt-axiom` umbrella crate instead unless you're implementing a new backend
adapter or inspecting generated circuits directly.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
