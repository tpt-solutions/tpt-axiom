# Changelog

All notable changes to `tpt-axiom-zk` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `claim` module: `ProofClaim<B: ZkBackend>` — the portable, data-only
  verification boundary between probabilistic computation and zero-knowledge
  proof (circuit name + public inputs + opaque proof, verified through any
  backend without leaking backend types upstream).
- `witness` module: backend-agnostic IR-level witness validation
  (`WitnessError::Arity` / `Violated`, `check`, `evaluate_nodes`) used by
  every backend to reject false witnesses at prove time.
- `conformance` feature exposing `conformance` drivers: the shared example
  circuits (`prove_balance_transfer`, `weighted_sum`, `bounds_check` with
  signed witnesses) and generic happy/rejection-path assertions every
  backend's test suite runs.

### Changed

- Crate renamed from `axiom-zk` to `tpt-axiom-zk`.

## [0.1.0] - Phase 2

### Added

- `CircuitDefinition` trait, implemented by `#[zk_provable]`-generated code.
- `ZkBackend` trait, the contract backend adapter crates implement.
