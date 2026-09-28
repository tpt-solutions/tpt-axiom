# Changelog

All notable changes to `tpt-axiom-ir` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Crate renamed from `axiom-ir` to `tpt-axiom-ir`.

## [0.1.0] - Phase 2

### Added

- `ConstraintSystemBuilder` / `Expr` / `Constraint` expression DAG.
- R1CS lowering (`r1cs.rs`) converting the IR into quadratic constraints.
- `Scalar = i64` placeholder field type.
