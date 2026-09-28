# Changelog

All notable changes to `tpt-axiom-backend-halo2` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Full `ZkBackend` implementation: PLONKish lowering of the IR
  (`circuit` module), real IPA key generation over Vesta, proving with
  Blake2b transcripts, and independent verification.
- Signed `i64` circuit semantics via bit-decomposition range checks on every
  named input and `NonNegative` constraint; `Halo2Params` (`range_bits`, `k`)
  carried in the backend `params` byte string with auto-sizing.
- IR-level witness validation at prove time via the shared
  `tpt_axiom_zk::witness` check, plus `with_witness_unchecked` for testing
  the circuit's own soundness.
- `MockProver`-backed tests, real prove/verify roundtrips and rejection
  paths (`tests/proofs.rs`); conformance-suite integration
  (`tests/conformance.rs`).
- Criterion benchmarks matching the arkworks backend's scenario IDs
  (`benches/backend.rs`).
- End-to-end demo: `examples/balance_transfer_prove.rs` and a separate
  `examples/balance_transfer_verify.rs` binary that verifies the shipped
  proof without any proving code.

### Changed

- Crate renamed from `axiom-backend-halo2` to `tpt-axiom-backend-halo2` (Phase 0).

## [0.1.0] - Phase 0

### Added

- Scaffolding: `Halo2Backend` unit struct with a `NAME` const. No real
  backend logic yet (targeted for Phase 4).
