# Changelog

All notable changes to `tpt-axiom-zk` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Crate renamed from `axiom-zk` to `tpt-axiom-zk`.

## [0.1.0] - Phase 2

### Added

- `CircuitDefinition` trait, implemented by `#[zk_provable]`-generated code.
- `ZkBackend` trait, the contract backend adapter crates implement.
