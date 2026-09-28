# tpt-axiom TODO

Probabilistic Logic and Native Zero-Knowledge State for Rust. Tracks all work for the whole project, phase by phase. See [spec.txt](spec.txt) for the design.

License: dual **MIT OR Apache-2.0**. Author: **TPT Solutions**.

## Phase 0: Project Scaffolding & Licensing

- [x] Initialize git repo and `.gitignore` (`/target`, etc.)
- [x] Create Cargo workspace (`resolver = "2"`) with `[workspace.package]` metadata: `license = "MIT OR Apache-2.0"`, `authors = ["TPT Solutions"]`, `edition`, `repository`, `homepage`, `documentation`, `keywords`, `categories`
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` files (copyright holder: TPT Solutions)
- [x] Scaffold crate layout under `crates/`:
  - [x] `tpt-axiom-core` — `Fuzzy<T>` / `Distribution<T>` probabilistic types
  - [x] `tpt-axiom-ir` — shared arithmetic intermediate representation (variance-propagation constraints + ZK circuit constraints)
  - [x] `tpt-axiom-macros` — `#[zk_provable]` proc-macro crate
  - [x] `tpt-axiom-zk` — `ZkBackend` trait + circuit/proving-key/verifying-key abstractions
  - [x] `tpt-axiom-backend-halo2`, `tpt-axiom-backend-arkworks`, `tpt-axiom-backend-sp1` — backend adapter crates (implemented in Phase 4, scaffolded as empty crates now)
  - [x] `tpt-axiom-cli` — build-time driver (key generation, verification invocation)
- [x] `tpt-axiom` umbrella crate re-exporting a `prelude` module matching the spec's `use tpt_axiom::prelude::*;`
- [x] README.md (overview, quickstart, links to spec)
- [x] ARCHITECTURE.md capturing the diagram in spec.txt §3
- [x] CONTRIBUTING.md
- [x] CHANGELOG.md
- [x] `.github/workflows/ci.yml` (fmt, clippy, test) — `tpt-telos` wasn't available locally to mirror, so this is a standard fmt/clippy/test workflow instead
- [x] `.github/ISSUE_TEMPLATE/` + `pull_request_template.md`
- [x] `examples/` directory with its own README

## Phase 1: Probabilistic & Uncertainty Types (Months 1-3)

- [x] Define `Fuzzy<T>` struct (mean + variance) for floating-point types
- [x] Define `Distribution<T>` as the more general uncertainty wrapper (start with Gaussian; leave room for other distributions later)
- [x] Implement `Fuzzy::new(mean, variance)` constructor and accessors
- [x] Operator overloading: `Add`, `Sub`, `Mul`, `Div` for `Fuzzy<T> + Fuzzy<T>`, `Fuzzy<T> + T`, and scalar variants, each applying the correct error-propagation formula
- [x] Implement common statistical helpers (confidence interval, standard deviation, z-score)
- [x] Unit tests validating propagated variance against known closed-form results
- [x] Property/Monte Carlo cross-check tests (sample many draws, compare empirical variance to analytically propagated variance within tolerance)
- [x] Sensor fusion / Kalman filter example in `examples/` using only standard arithmetic on `Fuzzy<T>`
- [x] **Milestone:** Kalman filter and Monte Carlo simulation examples compile and run using natural `+`/`*` syntax with correct propagated uncertainty. (Monte Carlo cross-checks live as tests in `tpt-axiom-core`; the standalone walkthrough landed as `crates/tpt-axiom/examples/monte_carlo.rs` in Phase 5.)

## Phase 2: ZK Arithmetic IR & `#[zk_provable]` Macro (Months 4-6)

- [x] Design backend-agnostic arithmetic IR (gate/constraint representation capable of lowering to R1CS and PLONKish forms) — `tpt-axiom-ir`'s `ConstraintSystem` expression DAG (`circuit.rs`) plus the `R1CS` lowering (`r1cs.rs`); a PLONKish gate lowering is left for the Phase 4 backend adapters, which is where a concrete PLONKish target (halo2) actually needs it
- [x] Define `ZkBackend` trait in `tpt-axiom-zk`: circuit representation, witness generation, proving-key generation, verifying-key generation, prove/verify calls
- [x] Implement `#[zk_provable(backend = "...")]` proc-macro in `tpt-axiom-macros`:
  - [x] Parse the annotated function's Rust AST (`syn`/`quote`)
  - [x] Distinguish `pub` (public input), plain (public output/local), and `secret` (witness) parameters
  - [x] Translate `assert!`/`assert_eq!` constraints in the function body into IR constraints
  - [x] Emit a clear compiler error for unsupported Rust constructs (loops with dynamic bounds, heap allocation, trait objects, etc.) — see `tests/ui/*.rs`
  - [x] Generate a circuit-definition type implementing a common `Circuit` trait consumed by any `ZkBackend`
- [x] Wire the macro's `backend = "halo2"` attribute argument to select the target `ZkBackend` impl at compile time
- [x] Unit tests: macro expansion snapshot tests + IR-correctness tests for simple arithmetic/comparison functions — `tests/circuits.rs` (IR correctness) and `tests/ui.rs` (trybuild compiler-error snapshots)
- [x] **Milestone:** `#[zk_provable]`-annotated `prove_balance_transfer` (from spec.txt §4) compiles and lowers to IR against the backend-agnostic trait, with no real proving backend wired in yet. (verified by `balance_transfer_lowers_to_ir` in `tests/circuits.rs`)

## Phase 3: Formal Verification of Circuits & Variance Formulas (Months 7-9)

- [x] ~~Add `tpt-telos` as a workspace dependency~~ — investigated and **superseded**. `tpt-telos`'s only reusable piece is `tpt-telos-verifier`, a QF_LRA (linear-arithmetic-only) solver; `tpt-axiom-ir` allows genuine bilinear terms (e.g. `a * k` for two witnesses) and `Fuzzy<T>`'s `*`/`/` variance formulas are nonlinear, both outside a linear solver's reach. Built `crates/tpt-axiom-verify` instead: a self-contained canonical-polynomial-normal-form checker, which handles the nonlinear cases a QF_LRA solver couldn't and has no external dependency. See that crate's README for the full rationale.
- [x] Bridge `Fuzzy<T>`/`Distribution<T>` operator-overload rules into a checkable form so variance-propagation arithmetic can be checked for soundness — `tpt-axiom-verify::fuzzy` covers `+`, `-`, `*`, `/`, and scalar scaling (more complete than a QF_LRA bridge could have reached, which would have been limited to `+`/`-`)
- [x] Bridge `tpt-axiom-ir` circuit constraints into a checkable form so the generated ZK circuit can be checked for equivalence against the original Rust function's asserted constraints — `tpt-axiom-verify::circuit::check_comparison` (exact polynomial identity) plus `evaluate` for concrete cross-checks against the kept-original Rust function (`tpt-axiom-macros/tests/circuits.rs`'s `*_ir_matches_rust` proptests)
- [x] Implement the "circuit mismatch" detector — as a library check + tests (`tpt-axiom-verify::circuit::check_comparison`), not a build-time gate: auto-discovering every `#[zk_provable]` function in a downstream crate needs a registry mechanism that doesn't exist yet; deferred to Phase 4, which needs similar discovery for key generation anyway. `tpt-axiom-cli verify` points at `cargo test -p tpt-axiom-verify` as today's answer.
- [x] Integration tests: a deliberately-broken `#[zk_provable]`-style circuit (an off-by-one, and swapped operands) is caught by verification with a useful diagnostic — `crates/tpt-axiom-verify/tests/circuit_mismatch.rs`
- [x] Integration tests: a deliberately-wrong variance-propagation formula is caught by verification — `crates/tpt-axiom-verify/tests/variance_mismatch.rs`
- [x] **Milestone (adjusted):** An intentionally introduced circuit-mismatch bug is caught by `cargo test -p tpt-axiom-verify` (not "the build" — no build-time gate exists yet, see above).

## Phase 4: ZK Backend Adapters (Months 10-12+)

- [x] `tpt-axiom-backend-halo2`: implement `ZkBackend` for halo2; generate real proving/verifying keys at build time; produce and verify a real proof for `prove_balance_transfer` — done against `halo2_proofs` 0.3 (IPA/Vesta): PLONKish lowering of the IR (selector gates + copy constraints + `assign_advice_from_constant` pins), bit-decomposition range checks enforcing signed `i64` semantics over the field (every named input proven in signed `range_bits`-bit range, every `NonNegative` in `[0, 2^range_bits)`), auto-sized real keygen, Blake2b-transcript proving, and independent verification. Because halo2's prover does not evaluate gates, prove also runs the shared `tpt_axiom_zk::witness` IR check so violating witnesses are rejected up front; `with_witness_unchecked` exists to test the circuit's own soundness.
- [x] `tpt-axiom-backend-arkworks`: implement `ZkBackend` for arkworks — Groth16 circuit-specific setup over BLS12-381, R1CS lowering of the IR (quadratic `Mul` gates, linear `+`/`-`/neg gates), same bit-decomposition range-check semantics, plus canonical key/proof serialization (`to_bytes`) with a wire-format roundtrip test.
- [x] `tpt-axiom-backend-sp1`: ~~implement~~ — investigated and **deferred** with the contract scaffolded. SP1 is a zkVM (RISC-V program execution), so a faithful adapter lowers the IR into a *program*, not a circuit; the `sp1-sdk` and `riscv32im-succinct-zkvm-elf` toolchain are only distributed for Linux/macOS, so nothing SP1-side can build or run here. `Sp1Backend` now implements the full `ZkBackend` contract with `Sp1Error::Unavailable` reporting the rationale; the planned design (IR→program lowering in a future `tpt-axiom-sp1-program` crate) is documented in the crate docs.
- [x] Backend conformance test suite: `tpt-axiom-zk`'s `conformance` feature exposes shared drivers (hand-built IRs mirroring the macro output for `prove_balance_transfer`, `weighted_sum`, `bounds_check` with signed negative witnesses) run through every backend, asserting identical prove/verify verdicts including tampered-public rejection and violating-witness prove failure (`tests/conformance.rs` in each adapter).
- [x] Benchmark suite comparing circuit size / proving time across backends — criterion benches with identical scenario IDs (`balance_transfer/{compile,keygen,prove,verify}` + a size report) in each adapter. Reference numbers on this repo's dev machine: halo2 k=9 (512 rows): keygen ≈ 17 ms, prove ≈ 7.7 ms, verify ≈ 2.6 ms; arkworks (264 R1CS constraints): keygen ≈ 414 ms, prove ≈ 338 ms, verify ≈ 10.2 ms.
- [x] **Milestone:** End-to-end demo — `examples/balance_transfer_prove.rs` produces a real halo2 proof (2,112 bytes for the demo transfer) and `examples/balance_transfer_verify.rs` verifies it in a separate process that runs no proving code (halo2 0.3 does not expose VK serialization, so the verifier regenerates the verifying key deterministically from the IR + `k`; arkworks-side keys/proofs *are* canonically serializable via `to_bytes`).

## Phase 5: Documentation, Examples & Release

- [x] Full API docs (`cargo doc`) for all public types, traits, and the `#[zk_provable]` macro — `cargo doc --no-deps --workspace` builds warning-free (broken intra-doc links fixed)
- [x] Expand `examples/`: Kalman filter (Phase 1), Monte Carlo simulation, sensor fusion, ZK balance-transfer (prove + separate verifier, Phase 4), ZK backend comparison — under `crates/tpt-axiom/examples/` and `crates/tpt-axiom-backend-halo2/examples/`, indexed from `examples/README.md`
- [x] Write a migration/getting-started guide in README.md — probabilistic API, `#[zk_provable]`, driving a `ZkBackend`, and a backend-choice table with reference benchmark numbers
- [ ] Publish crates to crates.io (`tpt-axiom-core`, `tpt-axiom-macros`, `tpt-axiom-zk`, backend adapters, `tpt-axiom` umbrella crate) — **pending owner decision/credentials**
- [x] Set up `docs.rs` documentation — `[package.metadata.docs.rs] all-features = true` on the feature-bearing crates; docs build warning-free so docs.rs will render cleanly on publish
- [ ] Tag `v0.1.0` release — deferred alongside the publish step (tagging before the crates.io version exists would misrepresent availability)

## AI & Probabilistic Intelligence Foundation

### Probabilistic Type System
- [ ] Define a first-class `Probability` type with validated [0,1] semantics.
- [ ] Define `Confidence` as a distinct semantic type from probability.
- [ ] Define `Distribution<T>` as a generic probabilistic value.
- [ ] Define `Uncertain<T>` for values with explicitly represented uncertainty.
- [ ] Define `Categorical<T>` for finite outcome distributions.
- [ ] Define `Bernoulli` and other fundamental distributions.
- [ ] Define `Score<T>` for probabilistic/ordinal scoring outputs.
- [ ] Define `Decision<T>` for typed probabilistic decisions.
- [ ] Define `Evidence<T>` for observations supporting probabilistic values.
- [ ] Define provenance metadata for probabilistic results.
- [ ] Ensure all core types are strongly typed, composable, serialisable and backend-independent.

### Uncertainty Operations
- [ ] Implement probability validation and normalisation.
- [ ] Implement distribution transformations.
- [ ] Implement uncertainty-preserving arithmetic.
- [ ] Implement probability and confidence propagation.
- [ ] Implement evidence combination.
- [ ] Implement conditional probability primitives.
- [ ] Implement Bayesian update primitives.
- [ ] Implement threshold and escalation primitives.
- [ ] Implement conversion from probabilistic results to deterministic decisions through explicit policies.
- [ ] Preserve uncertainty information unless explicitly discarded by the caller.

### AI Decision Types
- [ ] Define binary yes/no decision representation.
- [ ] Define categorical choice representation.
- [ ] Define ranking/selection representation.
- [ ] Define numerical scoring representation.
- [ ] Define multi-label decision representation.
- [ ] Define abstention/insufficient-confidence representation.
- [ ] Define competing-hypothesis representation.
- [ ] Define decision provenance and backend metadata.
- [ ] Define calibration metadata where available.

### Verification & Trust
- [ ] Define deterministic validation of all probabilistic outputs.
- [ ] Define reproducibility metadata.
- [ ] Define computation provenance.
- [ ] Define evidence provenance.
- [ ] Define verification boundaries between probabilistic and deterministic computation.
- [ ] Design a proof interface for verifiable probabilistic computations.
- [ ] Define interfaces suitable for future formal verification.
- [ ] Define interfaces suitable for future zero-knowledge verification.
- [ ] Ensure cryptographic/proof mechanisms remain optional and do not contaminate the core type system.

### Interoperability
- [ ] Define a stable serialisation format for probabilistic values and decisions.
- [ ] Define conversion interfaces for Augur.
- [ ] Define conversion interfaces for TPT inference runtimes.
- [ ] Define interfaces for external AI/decision engines.
- [ ] Ensure Axiom does not depend on any specific AI vendor, model, inference engine or network service.
- [ ] Add comprehensive property-based tests for probabilistic invariants.
- [ ] Add conformance tests for all probabilistic types.
- [ ] Document the mathematical and semantic meaning of every public type.