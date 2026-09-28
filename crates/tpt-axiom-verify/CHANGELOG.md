# Changelog

All notable changes to `tpt-axiom-verify` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - Phase 3

### Added

- `poly::Polynomial`: canonical multivariate polynomial normal form over
  `i64` coefficients, with exact equivalence checking.
- `circuit::check_comparison`: proves a `ConstraintSystem` constraint encodes
  the comparison it was claimed to.
- `circuit::evaluate`: concrete evaluation of a `ConstraintSystem` for
  cross-checking against the kept-original Rust function.
- `fuzzy`: algebraic soundness checks for `Fuzzy<T>`'s `+`, `-`, `*`, `/`, and
  scalar-scaling variance-propagation formulas.

### Design note

Built as a self-contained polynomial-identity checker instead of depending on
`tpt-telos` (a sibling formal-verification project): `tpt-telos-verifier` is
a QF_LRA (linear-arithmetic-only) solver, but `tpt-axiom-ir` allows genuine
bilinear terms and `Fuzzy<T>`'s `*`/`/` formulas are nonlinear — both outside
a linear solver's reach. See the crate README for the full rationale.
