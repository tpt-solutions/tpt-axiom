# Changelog

All notable changes to `tpt-axiom-backend-sp1` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `ZkBackend` contract implementation for SP1: `compile` retains the IR as
  the circuit handle, and key generation / proving / verification report
  `Sp1Error::Unavailable` with the platform rationale, pending the SP1
  toolchain (Linux/macOS-only) and the planned IR-to-program lowering.

### Changed

- Crate renamed from `axiom-backend-sp1` to `tpt-axiom-backend-sp1` (Phase 0).

## [0.1.0] - Phase 0

### Added

- Scaffolding: `Sp1Backend` unit struct with a `NAME` const. No backend
  logic yet.
