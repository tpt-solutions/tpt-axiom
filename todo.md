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
- [x] **Milestone:** Kalman filter and Monte Carlo simulation examples compile and run using natural `+`/`*` syntax with correct propagated uncertainty. (Monte Carlo cross-checks live as tests in `tpt-axiom-core`; a standalone `examples/monte_carlo.rs`-style walkthrough is still open for Phase 5's "expand examples" pass.)

## Phase 2: ZK Arithmetic IR & `#[zk_provable]` Macro (Months 4-6)

- [ ] Design backend-agnostic arithmetic IR (gate/constraint representation capable of lowering to R1CS and PLONKish forms)
- [ ] Define `ZkBackend` trait in `tpt-axiom-zk`: circuit representation, witness generation, proving-key generation, verifying-key generation, prove/verify calls
- [ ] Implement `#[zk_provable(backend = "...")]` proc-macro in `tpt-axiom-macros`:
  - [ ] Parse the annotated function's Rust AST (`syn`/`quote`)
  - [ ] Distinguish `pub` (public input), plain (public output/local), and `secret` (witness) parameters
  - [ ] Translate `assert!`/`assert_eq!` constraints in the function body into IR constraints
  - [ ] Emit a clear compiler error for unsupported Rust constructs (loops with dynamic bounds, heap allocation, trait objects, etc.)
  - [ ] Generate a circuit-definition type implementing a common `Circuit` trait consumed by any `ZkBackend`
- [ ] Wire the macro's `backend = "halo2"` attribute argument to select the target `ZkBackend` impl at compile time
- [ ] Unit tests: macro expansion snapshot tests + IR-correctness tests for simple arithmetic/comparison functions
- [ ] **Milestone:** `#[zk_provable]`-annotated `prove_balance_transfer` (from spec.txt §4) compiles and lowers to IR against the backend-agnostic trait, with no real proving backend wired in yet.

## Phase 3: `tpt-telos` Formal Verification Integration (Months 7-9)

- [ ] Add `tpt-telos` (`C:\Programming\2 WIP\tpt-telos`) as a workspace dependency (path dependency initially; revisit as a versioned/git dependency once tpt-axiom is ready to publish)
- [ ] Bridge `Fuzzy<T>`/`Distribution<T>` operator-overload rules into `tpt-telos`'s constraint format so variance-propagation arithmetic can be checked for soundness
- [ ] Bridge `tpt-axiom-ir` circuit constraints into `tpt-telos`'s verification format so the generated ZK circuit can be checked for equivalence against the original Rust function's asserted constraints
- [ ] Implement the "circuit mismatch" detector: a build-time (or `tpt-axiom-cli`) step that fails compilation when Rust logic and generated circuit diverge
- [ ] Integration tests: a deliberately-broken `#[zk_provable]` function (e.g. an off-by-one in the circuit lowering) is caught by verification and fails the build with a useful diagnostic
- [ ] Integration tests: a deliberately-wrong variance-propagation formula is caught by verification
- [ ] **Milestone:** An intentionally introduced circuit-mismatch bug is caught by `tpt-telos` verification before it reaches compiled output.

## Phase 4: ZK Backend Adapters (Months 10-12+)

- [ ] `tpt-axiom-backend-halo2`: implement `ZkBackend` for halo2; generate real proving/verifying keys at build time; produce and verify a real proof for `prove_balance_transfer`
- [ ] `tpt-axiom-backend-arkworks`: implement `ZkBackend` for arkworks
- [ ] `tpt-axiom-backend-sp1`: implement `ZkBackend` for sp1
- [ ] Backend conformance test suite: run the same set of `#[zk_provable]` example functions through every backend and assert consistent prove/verify results
- [ ] Benchmark suite comparing circuit size / proving time across backends
- [ ] **Milestone:** End-to-end demo — `prove_balance_transfer` produces a real halo2 proof that verifies independently of the proving process (e.g. via a separate verifier binary).

## Phase 5: Documentation, Examples & Release

- [ ] Full API docs (`cargo doc`) for all public types, traits, and the `#[zk_provable]` macro
- [ ] Expand `examples/`: Kalman filter, Monte Carlo simulation, sensor fusion, ZK balance-transfer, ZK backend comparison
- [ ] Write a migration/getting-started guide in README.md
- [ ] Publish crates to crates.io (`tpt-axiom-core`, `tpt-axiom-macros`, `tpt-axiom-zk`, backend adapters, `tpt-axiom` umbrella crate)
- [ ] Set up `docs.rs` documentation
- [ ] Tag `v0.1.0` release

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