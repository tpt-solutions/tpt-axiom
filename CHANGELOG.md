# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Browser proof verification: `tpt-axiom-wasm` grows a `verify` module with
  `groth16_verify`, running the real BLS12-381 Groth16 pairing check over a
  compressed verifying key and proof supplied as hex — so the playground can
  verify a real proof locally (proving stays native; the prover stack is not
  distributable to WASM). Malformed or truncated artifacts throw rather than
  reporting a clean `false`, and a public-input count that disagrees with the
  key is refused. The shipped demo proof (`result == a * k`, `k` secret, public
  `a = 2`, `result = 8`) verifies, and claiming `result = 9` does not. Seven
  native tests plus `docs/playground/smoke.mjs` (which drives the built
  WebAssembly module under Node) cover it; CI builds the wasm and runs the
  smoke test.
- Documentation: the mdBook gains a Cookbook chapter (end-to-end recipes for
  range claims, `!=`, branchy policies, unrolled accumulators, derived-
  statistic claims, envelope shipping, and failing-prove triage), and
  ARCHITECTURE.md gains a "Writing a backend" walkthrough of the full
  `ZkBackend` contract, including the free-witness tail convention and the
  shared conformance suite.
- Free-witness gadgets — the IR gains *free (aux) witness variables*
  (`ConstraintSystemBuilder::free_bool`): circuit-internal selector bits that
  are secret-visibility but deliberately not named inputs. The prove driver
  solves them by exhaustive search over their domain
  (`witness::solve_free_variables`, capped at 16 variables), and every
  backend range-checks them against their declared 1-bit type regardless.
  Built on that:
  - `assert!(a != b)` — the inequality gadget (booleanity + two gated range
    checks); a false equality satisfies neither arm, so it cannot prove.
  - `if` statements and `if`/`else` expressions — both arms lower and are
    gated by a selector pinned to the condition's truth; unselected
    constraints become identically-zero vacuities. All six comparison
    operators work as conditions.
  - bounded `for i in a..b` / `a..=b` loops (literal bounds, unrolled,
    capped at 1 024 iterations) with `let mut` accumulators
    (`acc += …` / `-=`, `*=` rebind as fresh SSA values).
  Free variables ride the secret-witness slice's tail; `NamedWitness` rejects
  them as inputs and `ir_digest` commits to them, so a de-gadgeted circuit
  can never re-verify a gadgeted proof.
- Phase C typed proving path: `#[zk_provable]` now generates a `<Fn>Inputs`
  struct carrying one field per parameter at its declared Rust type, with
  `new(..)`, `public_names()`, `secret_names()`, `named()` and, for returning
  circuits, `with_output(..)`.
- `tpt_axiom_zk::named::NamedWitness`: name-keyed witness assignment. Values
  resolve onto the positional form only after the map is checked complete
  against the circuit's IR, so a transposed or half-filled witness cannot
  restate the claim; an unsigned 64-bit value above the IR's `i64` scalar model
  is a named error, never a silent cast.
- `tpt_axiom_zk::layout::InputLayout`: the public-input layout printout — every
  slot's name, visibility, declared type, and whether it is the circuit output.
- `tpt_axiom_zk::options::KeygenOptions`: typed key-generation parameters with
  validated ranges, exposed as `ZkBackend::generate_keys_with_options` (the
  existing byte encoding is unchanged).
- `tpt_axiom_zk::registry` (behind the `registry` feature): a `linkme`-backed
  link-time circuit registry. `#[zk_provable(backend = "...", register)]`
  registers the circuit so a binary can enumerate it — what lets a
  `cargo axiom check` pass walk every circuit without a hand-maintained list.
  Registry entries rebuild their IR on demand, so they cannot go stale.
- `tpt-axiom-cli`: the real `cargo axiom` command surface — `doctor`,
  `inspect`, `check`, `keygen`, `prove`, and `verify`, with `--json` throughout,
  a library/binary split so the surface is unit-testable in-process, and exit
  codes that distinguish a clean negative from a tool error. `check` walks the
  circuit registry; `prove`/`verify` drive a real halo2 round trip through
  name-keyed JSON inputs and a `ProofEnvelope`.
- `tpt_axiom_zk::driver`: `keygen`, `prove_named`, `prove_ir_named`, and
  `verify_claim` — one checked path from a typed input struct to a verified
  proof, with IR validation, witness resolution, and the constraint check
  performed before any proving work.

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
- Proof portability: `tpt_axiom_zk::ProofEnvelope`, a version-tagged wire
  format carrying the backend, circuit, public inputs, the IR-digest binding
  and the canonical proof bytes, with `ProofClaim::to_envelope`/`from_envelope`
  round-tripping a claim through JSON without loosening its circuit binding.
  Backends opt in via new `ZkBackend::encode_proof`/`decode_proof` hooks;
  `arkworks::from_bytes` is the inverse of `to_bytes` for shipped key material
  and rejects trailing bytes.
- `tpt-axiom-core` distributions: `Uniform`, `Beta`, `Gamma`, `LogNormal`,
  `StudentT`, `Poisson`, `Binomial` with `pdf`/`cdf`/`quantile`/
  `prob_greater_than`, on new dependency-free special functions
  (`lgamma`, `gamma_p`/`gamma_q`, `beta_reg`), plus conjugate updates and
  `kl_gaussian`.
- `tpt-axiom-core::logits` — temperature-scaled `softmax`/`log_softmax`,
  `cross_entropy`, `top_k`.
- Nonlinear `Fuzzy` transforms (`exp`, `ln`, `sqrt`, `powi`, `tanh`) via a
  generic `Fuzzy::transform` engine with second-order mean correction, and
  checked N-way fusion (`Fuzzy::fuse_all`).
- `tpt-axiom::audit::ProofCarriedDecision` — a `DecisionRecord` bundled with
  the proof that justifies it, re-verifiable by an auditor against their own
  copy of the circuit definition.
- `examples/age_proof.rs` (halo2) — prove you are 18+ without revealing your
  age or birth year, including the adversarial underage witness; run in CI
  with its documented expected output.
- AI Foundation (Interoperability): new `tpt-axiom-interop` crate converting
  `tpt-augur`'s `Dist` family to Axiom uncertainty types (exact closed-form
  moment matching, validated, round-trippable for `Normal`) and defining the
  `InferenceSample` boundary contract for the TPT inference runtimes
  (`tpt-gpu`/`tpt-local-ai`/`tpt-spark`, pending their crates.io debut).
- AI Foundation (Interoperability): vendor-neutral interfaces for external
  AI/decision engines — `tpt-axiom-interop::engine`'s `DecisionEngine`
  trait plus validated boundary types (`EngineOutput`, `EngineVerdict`,
  `MultiLabelOutput`) that turn raw engine scores (probabilities or
  logits, stable temperature softmax) into Axiom decisions under
  caller-owned thresholds, with provenance; no vendor/SDK/network
  dependencies anywhere.
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
