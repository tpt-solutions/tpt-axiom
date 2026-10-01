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
- Per-input range checks keyed on each variable's declared
  `tpt_axiom_ir::IntType`: signed types shift through the `signed-shift` gate
  by `2^(bits-1)`, unsigned types are checked directly, so a `u8` parameter is
  proven in `[0, 2^8)`. `auto_k` sizes the row budget from the declared
  widths.
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
- `examples/age_proof.rs`: the README-lead example — prove `age >= 18`
  without revealing the birth year, including the adversarial underage
  witness the prover must refuse. Documented expected output, run in CI.
- `encode_proof`/`decode_proof` (the proof already *is* its Blake2b-transcript
  byte string), so claims port through `tpt_axiom_zk::ProofEnvelope`; an
  envelope-roundtrip test asserts the rebuilt claim still verifies against its
  own circuit and is refused against a different one.

### Changed

- Crate renamed from `axiom-backend-halo2` to `tpt-axiom-backend-halo2` (Phase 0).

## [0.1.0] - Phase 0

### Added

- Scaffolding: `Halo2Backend` unit struct with a `NAME` const. No real
  backend logic yet (targeted for Phase 4).
