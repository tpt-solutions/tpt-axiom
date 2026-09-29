# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Phase 4: `tpt-axiom-backend-halo2` — real `ZkBackend` for halo2
  (`halo2_proofs` 0.3, IPA/Vesta): PLONKish lowering of the IR with selector
  gates and copy constraints, bit-decomposition range checks giving sound
  signed `i64` semantics, auto-sized real key generation, Blake2b-transcript
  proving, and verification. Includes `MockProver` + roundtrip/rejection
  tests, criterion benchmarks, and an end-to-end demo where a separate
  verifier binary verifies the shipped proof without any proving code.
- Phase 4: `tpt-axiom-backend-arkworks` — real `ZkBackend` for arkworks
  (Groth16 over BLS12-381): R1CS lowering of the IR, circuit-specific setup,
  canonical key/proof serialization, roundtrip/rejection tests, conformance
  integration, and criterion benchmarks.
- Phase 4: shared backend conformance suite (`tpt-axiom-zk` `conformance`
  feature) running identical example circuits — including signed negative
  witnesses — through every backend with identical prove/verify verdicts.
- Phase 4: `tpt_axiom_zk::witness` — backend-agnostic IR-level witness
  validation rejecting violating witnesses at prove time with a precise
  constraint index.
- Phase 4: `tpt-axiom-backend-sp1` — `ZkBackend` contract implemented with
  `Sp1Error::Unavailable` reporting; proving deferred until the SP1 SDK and
  RISC-V toolchain (Linux/macOS-only) are available.
- Phase 4: criterion benchmark suites with matching scenario IDs across
  backends (circuit size, keygen, prove, verify for `prove_balance_transfer`).
- AI Foundation (Interoperability): new `tpt-axiom-interop` crate converting
  `tpt-augur`'s `Dist` family to Axiom uncertainty types (exact closed-form
  moment matching, validated, round-trippable for `Normal`) and defining the
  `InferenceSample` boundary contract for the TPT inference runtimes
  (`tpt-gpu`/`tpt-local-ai`/`tpt-spark`, pending their crates.io debut).
- AI Foundation (Probabilistic Type System): `tpt-axiom-core::intelligence`
  with `Probability`, `Confidence`, `Bernoulli`, `Categorical<T>`,
  `Uncertain<T>`, `Score<T>`, `Decision<T>` (first-class abstention),
  `Evidence<T>`, and `Provenance`; opt-in `serde` serialisation; all exposed
  through the `tpt-axiom` prelude.
- Phase 0: Cargo workspace scaffolding for `tpt-axiom-core`, `tpt-axiom-ir`,
  `tpt-axiom-macros`, `tpt-axiom-zk`, `tpt-axiom-backend-halo2`, `tpt-axiom-backend-arkworks`,
  `tpt-axiom-backend-sp1`, `tpt-axiom-cli`, and the `tpt-axiom` umbrella crate.
- Phase 0: `LICENSE-MIT` / `LICENSE-APACHE`, `README.md`, `ARCHITECTURE.md`,
  `CONTRIBUTING.md`, CI workflow, issue/PR templates, `examples/`.
- Phase 1: `Fuzzy<T>` probabilistic type (mean + variance) with `Add`, `Sub`,
  `Mul`, `Div` operator overloads (both `Fuzzy<T> op Fuzzy<T>` and
  `Fuzzy<T> op T`) implementing error-propagation arithmetic, plus
  `confidence_interval`, `std_dev`, and `z_score` helpers.
- Phase 1: `Distribution<T>` general uncertainty wrapper (Gaussian today).
- Phase 1: closed-form unit tests and a Monte Carlo cross-check test suite
  for `Fuzzy<T>` arithmetic.
- Phase 1: Kalman-filter example (`cargo run -p tpt-axiom --example kalman_filter`).
