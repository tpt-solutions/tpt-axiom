# tpt-axiom-verify

Polynomial-identity verification of `#[zk_provable]` circuits and `Fuzzy<T>`
variance-propagation formulas for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) — `no_std`.

`tpt-axiom-ir`'s expression DAG (constants, variables, `+`, `-`, `*`, unary
`-`) and `Fuzzy<T>`'s variance formulas are both polynomials in their inputs.
Normalizing an expression into a canonical multivariate `Polynomial` gives
*exact*, decidable equivalence checking — including bilinear/quadratic terms
(e.g. `a * k` for two witnesses) that a linear-arithmetic-only SMT solver
cannot express — with no external dependency.

- [`circuit::check_comparison`] proves that a `Constraint` in a
  `ConstraintSystem` encodes the comparison (`>=`, `<=`, `>`, `<`, `==`) it
  was claimed to, catching a `#[zk_provable]` lowering bug (an off-by-one, or
  swapped operands) — including for nonlinear circuits.
- [`circuit::evaluate`] concretely evaluates a `ConstraintSystem`, for
  cross-checking the generated IR against the kept-original Rust function on
  sampled inputs (see `tpt-axiom-macros`'s `tests/circuits.rs`).
- [`fuzzy`] checks `Fuzzy<T>`'s `+`, `-`, `*`, `/`, and scalar-scaling
  variance-propagation formulas against their textbook identities — see that
  module's docs for exactly what this does and doesn't prove (it's a
  hand-transcription check against a reference identity, not automatic
  verification of `tpt-axiom-core`'s Rust source).

## Why not a general SMT solver?

An earlier draft of this crate depended on `tpt-telos`'s QF_LRA (linear
arithmetic only) verifier. `tpt-axiom-ir` allows genuine bilinear terms
(`a * k` for two witnesses) and `Fuzzy<T>`'s multiplication/division formulas
are nonlinear in their inputs — both outside a linear solver's reach. Since
every proof obligation here is a polynomial identity rather than general
satisfiability, a canonical polynomial normal form is *strictly more
capable* for this domain and has no external dependency.

## Status

Library-only (Phase 3). Auto-discovering every `#[zk_provable]` function in a
downstream crate for a build-time gate (`axiom verify`) is left for Phase 4,
alongside the backend adapters' similar discovery needs for key generation.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
