# Changelog

All notable changes to `tpt-axiom-backend-arkworks` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Full `ZkBackend` implementation: R1CS lowering of the IR (`circuit`
  module), Groth16 circuit-specific setup over BLS12-381, proving, and
  verification.
- Signed `i64` circuit semantics via bit-decomposition range checks on every
  named input and `NonNegative` constraint; `ArkworksParams` (`range_bits`)
  carried in the backend `params` byte string.
- Per-input range checks keyed on each variable's declared
  `tpt_axiom_ir::IntType`: signed types are shifted by `2^(bits-1)` into
  `[0, 2^bits)`, unsigned types are checked directly against the input, so a
  `u8` parameter is proven in `[0, 2^8)`.
- IR-level witness validation at prove time via the shared
  `tpt_axiom_zk::witness` check.
- Canonical serialization helpers (`to_bytes`/`from_bytes`) for keys and
  proofs; `from_bytes` refuses trailing bytes rather than accepting a prefix
  of a truncated or concatenated artifact. Roundtrip tests cover both.
- `encode_proof`/`decode_proof` (compressed `CanonicalSerialize`) so claims can
  be ported through `tpt_axiom_zk::ProofEnvelope`; `ArkworksProof` is now
  `PartialEq` so envelope roundtrips are comparable.
- Groth16 roundtrip/rejection tests (`tests/proofs.rs`); conformance-suite
  integration (`tests/conformance.rs`).
- Criterion benchmarks matching the halo2 backend's scenario IDs
  (`benches/backend.rs`).

### Changed

- Crate renamed from `axiom-backend-arkworks` to `tpt-axiom-backend-arkworks` (Phase 0).

## [0.1.0] - Phase 0

### Added

- Scaffolding: `ArkworksBackend` unit struct with a `NAME` const. No real
  backend logic yet (targeted for Phase 4).
