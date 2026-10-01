# Changelog

All notable changes to `tpt-axiom` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `audit` module: `ProofCarriedDecision<T, B>` — a `DecisionRecord<T>` bundled
  with the `ProofClaim<B>` that justifies it. `verify` re-checks the carried
  claim against the auditor's own circuit definition (so a weakened
  redefinition is a clean `false`), and `claimed_publics` exposes the exact
  statement the decision was computed from. Covered end to end in
  `tests/audit.rs` against a real halo2 proof.

### Changed

- Sub-crate dependencies updated to the renamed `tpt-axiom-core`,
  `tpt-axiom-ir`, `tpt-axiom-macros`, `tpt-axiom-zk` package names.

## [0.1.0] - Phase 1

### Added

- `prelude` module re-exporting `Fuzzy`, `Distribution`, `stats`,
  `ConstraintSystem`, `ConstraintSystemBuilder`, `Scalar`, `zk_provable`,
  `CircuitDefinition`, and `ZkBackend`.
- `examples/kalman_filter.rs`.
