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
- [x] Define a first-class `Probability` type with validated [0,1] semantics — `tpt-axiom-core::intelligence`: fallible `new` / panicking `new_unchecked`, `complement`, weighted `pooled`, no implicit lossy arithmetic.
- [x] Define `Confidence` as a distinct semantic type from probability — same range, separate type; crossing requires the explicit `into_probability()` so the semantic leap is visible at the call site.
- [x] Define `Distribution<T>` as a generic probabilistic value — the Phase 1 `tpt-axiom-core::Distribution` (Gaussian/constant) already provides this.
- [x] Define `Uncertain<T>` for values with explicitly represented uncertainty — `Certain(T)` / `Estimated(Fuzzy<T>)`; uncertainty is preserved through `map` and only dropped by the explicit `point_estimate()`.
- [x] Define `Categorical<T>` for finite outcome distributions — weight-normalizing construction, `probability_of`, `most_likely`, `entropy_bits`, sampling.
- [x] Define `Bernoulli` and other fundamental distributions — `Bernoulli` (likelihood, odds, sampling) here; further families ride the existing `Distribution<T>` enum as needed.
- [x] Define `Score<T>` for probabilistic/ordinal scoring outputs — value + `Confidence` pairs.
- [x] Define `Decision<T>` for typed probabilistic decisions — `Commit { value, confidence }` / `Abstain { reason }`, with `Decision::from_score(score, threshold)` as the explicit policy conversion (abstention is first-class).
- [x] Define `Evidence<T>` for observations supporting probabilistic values — likelihood-weighted observations with multiplicative `combine`.
- [x] Define provenance metadata for probabilistic results — `Provenance` (origin, timestamp, revision), carried on `Evidence`, vendor- and backend-independent.
- [x] Ensure all core types are strongly typed, composable, serialisable and backend-independent — no crypto/AI-vendor dependencies; opt-in `serde` feature (derive) on `tpt-axiom-core` serialises every intelligence type; tested with and without the feature.

### Uncertainty Operations
- [x] Implement probability validation and normalisation — `Probability::new`/`new_unchecked`, `Categorical::new` normalizing construction, and the `Validate` trait re-checking both.
- [x] Implement distribution transformations — `Distribution::affine` (exact for Gaussians) and `map_first_order` (delta-method approximation).
- [x] Implement uncertainty-preserving arithmetic — `Fuzzy<T>` operator overloads plus `Uncertain<T>::map` (first-order propagation) that never discards spread.
- [x] Implement probability and confidence propagation — `Probability::{noisy_or, conjunct, pooled}`, `Confidence::{conjunct, disjunct}`.
- [x] Implement evidence combination — `Evidence::{combine, combine_all}` (multiplicative, provenance-preserving).
- [x] Implement conditional probability primitives — likelihood application `P(E|H)` is the engine of `Categorical::bayesian_update` / `Hypotheses::revise`; joint/marginal chances via `noisy_or`/`conjunct`.
- [x] Implement Bayesian update primitives — `Categorical::bayesian_update` (posterior over reweighted prior) and `Hypotheses::revise`.
- [x] Implement threshold and escalation primitives — `classify_by_confidence` + `Escalation` (accept/review/reject bands, policy parameters owned by the caller).
- [x] Implement conversion from probabilistic results to deterministic decisions through explicit policies — `Decision::from_score`, `Ranking::decide`, `Categorical::decide` (thresholds always caller-supplied).
- [x] Preserve uncertainty information unless explicitly discarded by the caller — enforced by `Uncertain<T>`: only `point_estimate()` drops spread, and the name marks it.

### AI Decision Types
- [x] Define binary yes/no decision representation — `BinaryDecision` (`Decision<bool>`) with `Decision::yes`/`no` constructors.
- [x] Define categorical choice representation — `Categorical::decide(threshold)` committing to the most likely outcome.
- [x] Define ranking/selection representation — `Ranking<T>` (best-first scored candidates) with `decide`.
- [x] Define numerical scoring representation — `Score<T>` (value + `Confidence`).
- [x] Define multi-label decision representation — `MultiLabelDecision` (per-label binary decisions; uncertainty never leaks between labels).
- [x] Define abstention/insufficient-confidence representation — `Decision::Abstain { reason }` with `AbstentionReason`.
- [x] Define competing-hypothesis representation — `Hypotheses<T>` (normalized posterior + `revise` + `leader`).
- [x] Define decision provenance and backend metadata — `DecisionRecord<T>` bundling a decision with `Provenance`.
- [x] Define calibration metadata where available — `Calibration` (method, reference, expected calibration error, validated).

### Verification & Trust
- [x] Define deterministic validation of all probabilistic outputs — the `Validate` trait (range, normalization re-checks) implemented for `Probability`, `Confidence`, `Categorical`.
- [x] Define reproducibility metadata — `Reproducibility` (algorithm, version, RNG seed).
- [x] Define computation provenance — `Provenance` (origin/timestamp/revision), plus `Reproducibility` for re-run data.
- [x] Define evidence provenance — every `Evidence` carries its own `Provenance`.
- [x] Define verification boundaries between probabilistic and deterministic computation — `Validate` (trust-free invariant checks) on one side, `tpt_axiom_zk::claim::ProofClaim` (proof-carrying statements) on the other; nothing crosses implicitly.
- [x] Design a proof interface for verifiable probabilistic computations — `ProofClaim<B: ZkBackend>`: circuit name + public inputs + opaque proof, verified via `verify_with(backend, vk)`.
- [x] Define interfaces suitable for future formal verification — the intelligence types are pure, total functions over validated data with documented invariants (the same property `tpt-axiom-verify` audits for variance formulas).
- [x] Define interfaces suitable for future zero-knowledge verification — `ProofClaim` works unchanged over any future `ZkBackend` implementation (sp1's contract is already in place).
- [x] Ensure cryptographic/proof mechanisms remain optional and do not contaminate the core type system — `tpt-axiom-core` has no crypto dependencies; `ProofClaim` is data-only and lives behind `tpt-axiom-zk`.

### Interoperability
- [x] Define a stable serialisation format for probabilistic values and decisions — opt-in `serde` with the default representations pinned as wire-format v1 by roundtrip tests (`tests/properties.rs::serde_roundtrips_pin_the_v1_wire_format`).
- [x] Define conversion interfaces for Augur — `tpt-augur` is public (`tpt-solutions/tpt-augur`) and `tpt-augur-std 0.1.0` is on crates.io; `tpt-axiom-interop::augur` converts its `Dist` family into Axiom types via exact closed-form moment matching (information-preserving for `Normal`), with parameter validation and the exact `Normal` inverse.
- [x] Define conversion interfaces for TPT inference runtimes — `tpt-axiom-interop::inference` fixes the contract (`InferenceSample`: sampled value + log-space weight → `Evidence`/`Score`); the runtimes (`tpt-gpu`, `tpt-local-ai`, `tpt-spark`) are not on crates.io yet, so their feature-gated `From` impls land there when they publish.
- [x] Define interfaces for external AI/decision engines — `tpt-axiom-interop::engine`: the vendor-neutral `DecisionEngine` trait (the only seam an engine implements; it performs no I/O and adds no vendor/SDK/network dependency — an adapter crate implements it against its vendor SDK), plus validated boundary types `EngineOutput<T>` (probability or logit scores over mutually exclusive outcomes; numerically stable temperature softmax), `EngineVerdict` (single binary score via the logistic function), and `MultiLabelOutput` (independent per-label scores), each converting into Axiom decision types only through caller-supplied thresholds, with `Provenance` attached for downstream `DecisionRecord`s.
- [x] Ensure Axiom does not depend on any specific AI vendor, model, inference engine or network service — by construction: the foundation is pure `no_std` math and metadata with zero vendor/network dependencies.
- [x] Add comprehensive property-based tests for probabilistic invariants — `crates/tpt-axiom-core/tests/properties.rs` (proptest: complement partitioning, noisy-OR/conjunct domination, categorical normalization + entropy bounds, evidence-weight closure, policy/threshold agreement, Bayesian renormalization).
- [x] Add conformance tests for all probabilistic types — unit suites per type in `intelligence`/`decision_types`, plus the invariant properties above and the wire-format roundtrips.
- [x] Document the mathematical and semantic meaning of every public type — module docs state each type's semantics (probability vs confidence distinction, first-order propagation limits, policy ownership); `cargo doc` builds warning-free.


## Platform Review Follow-ups (2026-10-01)

Findings come from a read-only code review; reproduce each bug with a failing test before fixing. Phases A-E below.

### Phase A: Correctness & Soundness
#### A1. ZK soundness
- [x] Carry declared integer type `(bits, signed)` per variable into the IR and use it in halo2/arkworks range checks (previously every parameter was treated as signed i64, so `u8`/`u64` params accepted out-of-width values) — `IntType` on `VariableInfo`, filled in from each parameter's Rust type by `lower.rs`, consumed by halo2's `range_check_input`, arkworks' named-input loop, and `auto_k`'s row budget; a `u8` parameter is now proven in `[0, 2^8)` (regression tests in both backends' `tests/proofs.rs`)
- [x] Enforce `let x: u8 = ...` type ascriptions (previously dropped silently): non-integer ascriptions are rejected, and an ascription that contradicts the declared type of the binding it initialises is a compile error (`tests/ui/let_type_mismatch.rs`)
- [x] Range-check intermediate `Mul` results — implemented as the static-bounds option: `ConstraintSystem::bit_bounds` tracks each node's worst-case width (declared input types, one carry bit per `+`/`-`, width sum per `*`), and `validate()` rejects any circuit whose intermediate can reach `MAX_FAITHFUL_BITS` (250 ≪ the ~254-bit backend fields), so a field image can never wrap past integer semantics (`IntermediateOverflow`, keygen/compile-time rejection tests in both backends)
- [x] Evaluate witnesses in `i128` instead of wrapping `i64` (`zk/witness.rs`) — exact checked `i128` evaluation shared by all backends via `witness::evaluate_nodes`; new precise errors `Overflow` (i128 exceeded), `InputOutOfRange` (input outside its declared `IntType`), `NonNegativeOutOfRange` (value in `[2^range_bits, ∞)`, matching what the bit-decomposition chain can prove; `check_with_range` takes the backend's configured width). The halo2/arkworks field assignments encode `i128` exactly (`encode_i128`; every `i128` is distinct mod p)
- [x] Make non-final `return;` a compile error (`lower.rs`) — any `return`/`return <expr>;` that is not the function's final statement is rejected (`tests/ui/early_return.rs`)
- [x] Support `&&` inside `assert!` and add `debug_assert*` (`lower.rs`) — conjunctions lower each comparison to its own constraint (semantics cross-checked with `tpt-axiom-verify::check_comparison` in `tests/circuits.rs`); `debug_assert!`/`debug_assert_eq!` lower identically to `assert!`/`assert_eq!` (a circuit has no debug builds); `||` gets a dedicated error
- [x] `verify` returns `Ok(false)` (not `Err`) on wrong public-input count (halo2 + arkworks) — halo2's `Halo2VerifyingMaterial` now carries `num_publics` (halo2 vk cannot recover it); arkworks derives the count from `vk.gamma_abc_g1.len() - 1`; both check before invoking the backend (`tests/proofs.rs`)
- [x] `ProofClaim` commits to an IR digest and `verify_with` checks it — `tpt_axiom_zk::ir_digest` SHA-256s a canonical encoding of the full IR (names, types, expression table, slots, constraints); `ProofClaim::new(circuit, ir, publics, proof)` stores it and `verify_with(backend, vk, ir)` returns `Ok(false)` on any mismatch, so a proof can never be replayed against a different circuit. (Commits to the *IR* rather than the vk: halo2 0.3 vk bytes are not portable, and the IR is what determines the proved statement.)
- [x] Document Groth16 per-circuit trusted setup / toxic waste — new "Groth16 trusted setup" section in the arkworks crate docs (in-process `OsRng`, dev/test-suitable, production needs a ceremony)
- [x] Fix range-bits docs ("max 255" vs actual cap of 64) — halo2 `Halo2Params` doc now states the 64-bit cap (the arkworks doc already did)
- [x] `ConstraintSystem::validate()` + return `Result` instead of `expect` on malformed IR — `validate()` checks structure (range/forward-reference/DAG rules, input-slot/visibility consistency, var-map consistency) plus the bit-width bound; `lower_r1cs` returns `Result<R1CS, CircuitError>`, `R1CS::evaluate` returns typed errors for malformed witnesses (`WitnessLength`/`ConstantSlot`/`Malformed`) instead of panicking, and every backend validates at `compile`/`generate_keys`. (Fields intentionally stay public: builder-only construction already makes structural malformation unrepresentable in practice, and privatizing ~100 field touch points across five crates buys little beyond what `validate()` now guarantees.)
- [x] Checked/i128 arithmetic in `verify/poly.rs` (i128 coefficients), `verify/circuit.rs::evaluate` (exact i128 internally, documented two's-complement narrowing to the i64 scalar model), `ir/r1cs.rs::evaluate` (i128 sums/checked products; overflow ⇒ gate failure, never a wrap or a debug panic)
- [x] `verify/fuzzy.rs` add/sub checks are tautological — dropped `check_add_variance`/`check_sub_variance` (a transcription of a two-term sum can only ever equal itself, so those checks verified nothing); the module doc now says so explicitly and points at the Monte Carlo cross-checks as the empirical backstop
- [x] Precompute variable-to-expression map (O(n²) lookups in backends/witness) — `ConstraintSystem::var_exprs` maintained by the builder, `var_expr_id` accessor; witness evaluation, halo2's assign/range-check passes and arkworks' range-check pass all use it (plus a precomputed public-variable→instance-row table in halo2)
- [x] Negative tests for all of the above (adversarial witnesses via `with_witness_unchecked` in both backends, trybuild `early_return`, malformed-IR/validate tests in `tpt-axiom-ir`, wrong-count/overflow/keygen-rejection tests in backend `tests/proofs.rs`)

#### A2. Core numerics (`tpt-axiom-core`)
- [x] Serde deserialization enforces invariants — custom `Deserialize` (shadow-struct / tuple re-validation) for `Probability`, `Confidence`, `Fuzzy`, `Categorical`, `Evidence`; nested types (`Bernoulli`, `Score`, `Decision`, `Uncertain::Estimated`) are safe through their validated inner values; negative-payload suite in `tests/serde_invariants.rs` (out-of-range chances, negative variance, non-normalized categorical, non-positive weights all rejected)
- [x] `Fuzzy` `/` and `*` by exact zero/inf no longer panic on NaN variance — operators go through the unchecked internal `new_raw` (IEEE-faithful degenerate results), plus `Fuzzy::checked_div` (Err with the rejected divisor) and documented `FuseError` for the validated paths
- [x] `fuse` with zero variances — IEEE `0/0` through `new_raw`; `checked_fuse` rejects degenerate fusion (`Err(FuseError)`)
- [x] `norm_ppf` panic for confidence near 1 — `confidence_interval` now computes `0.5 + c/2` (never rounds to 1.0 below the last few ulps) and clamps the tail defensively
- [x] `Distribution::sample` no longer panics on a 0.0/1.0 RNG draw — draws clamp into the open interval, mapping to the corresponding extreme quantile
- [x] `norm_ppf` far-tail + accurate `erfc`-based `erf`/`norm_cdf` — `erfc` is a power series (< 2) + continued fraction (≥ 2, A&S 7.1.14 via modified Lentz) at ~1e-15 *relative* accuracy into the far tail; `norm_ppf` = Acklam start + log-space Newton against the full-precision CDF (round-trips at p = 1e-300); the "1.15e-9" claim is now scoped to the Acklam initial estimate it describes
- [x] `num-traits` `std` made optional — core gained `std` (default) / `libm` features; `cargo build --no-default-features --features libm` is the true-`no_std` proof (quants routes through `num_traits::Float` there). thumbv7em CI check deferred to the Phase B CI item
- [x] Checked constructors — `Distribution::try_gaussian` (+ panicking `gaussian` now validating), `Calibration::new`. `Provenance` deliberately has none: its fields are free-form strings/options with no invariant to enforce
- [x] Zero-variance `Gaussian` normalizes to `Constant`; `Fuzzy::from(Distribution)` replaces the `TryFrom`+`NotGaussian` pair (a `Constant` is a zero-variance `Fuzzy` — the conversion was always total)
- [x] `Categorical::new_strict` — errors on NaN/zero/negative weights (`InvalidWeight`), distinct `Overflow` error, `Empty` otherwise; tolerant `new` unchanged and documented as the dropping constructor
- [x] `Evidence` stores log-weights — `from_log_weight` constructor, exact `log_weight()` accessor, `combine`/`combine_all` now infallible log addition (linear `weight()` reads back `inf` instead of failing); `InferenceSample::to_evidence` transfers the runtime's log weight exactly (Evidence wire format moves to the `log_weight` field, pinned in the roundtrip test)
- [x] `Probability::new(-0.0)` normalizes to `+0.0`; panicking `new_unchecked` renamed to `new_or_panic` on both `Probability` and `Confidence` (the name now says what it does)
- [x] `Confidence` gained `Ord`/`Eq` (total_cmp-based)/`Display`/`Default` (NONE); `Probability` gained `Default` (ZERO)
- [x] Dropped derived `Eq` on float-backed types — `Fuzzy` and `Distribution` are now `PartialEq`-only with a doc reason (`Probability`/`Confidence` keep `Eq`: validated range means no NaN, and `-0.0` is normalized)
- [x] Docs — division row marked as the delta-method approximation (not "exact"), loud independence warning with the `x - x` has-variance-`2v` example, `z_score` zero-sigma behavior documented, stale `tpt-telos` mention removed from `verify` claims

#### A3. Interop
- [x] Augur bridge accepts `sigma == 0` (maps to `Distribution::Constant` — the round trip through `from_distribution` is now lossless) and validates the variance *after* squaring (`sigma = 1e200` is rejected as an overflow, not stored as an infinite variance)
- [x] Non-Normal families renamed to `*_approx` — `to_fuzzy_approx`/`to_uncertain_approx` (docs state exactly what is lost: higher moments; only `Normal` is information-preserving)
- [x] `variance.max(0.0)` NaN-swallowing removed — `from_distribution`/`from_fuzzy` return `Result` and reject non-finite moments or negative variance instead of silently emitting a zero-sigma normal
- [x] Duplicate labels rejected in `MultiLabelOutput::from_probabilities` (`EngineError::DuplicateLabel` — two scores for one label is exactly the uncertainty-leak the per-label independence contract forbids)
- [x] `active_at <= 0.5` rejected in `threshold_decision` (`EngineError::InvalidThreshold`): at a coin-flip threshold the abstention band is empty and `p == 0.5` would be *committed*; `EngineVerdict::decide`/`MultiLabelOutput::decide` propagate

### Phase B: Credibility & Adoption Blockers
- [x] Verified: README headline output is wrong (variance 0.6 gives sigma 0.775, not 0.742)
- [ ] Fix README headline output, clarify variance vs std-dev; add `Fuzzy::from_std_dev`
- [ ] Make README a doctest (`#![doc = include_str!("../README.md")]`); fix section 3 snippet; document the intelligence/AI-foundation types; replace "Phase N" framing with works-today / experimental / missing
- [ ] `SECURITY.md`, not-audited banner, threat model in `ARCHITECTURE.md`
- [ ] Umbrella crate feature flags (`halo2`, `arkworks`, `interop`, `verify`) + grouped prelude
- [ ] crates.io readiness: `readme =` on each crate, confirm `tpt-augur-std` availability, remove unused CLI deps, `publish = false` on sp1/stubs, drop "Phase 4" from descriptions, remove stale `tpt-telos` mentions, `cargo-axiom` binary name, release-plz publish order (ir, core, zk, macros, verify, backends, umbrella)
- [ ] Repo hygiene: commit `Cargo.lock`; add `CODE_OF_CONDUCT.md`, `CITATION.cff`, `CODEOWNERS`, `deny.toml`, `rust-toolchain.toml`, `dependabot.yml`, issue-template `config.yml`; untrack `.kilo`/`.mimocode`; user-facing `ROADMAP.md`; move `spec.txt` to `docs/design/`; single changelog strategy
- [ ] CI: MSRV 1.85, Windows/macOS matrix, cargo-deny + audit, `-D warnings` docs (all-features), semver-checks, llvm-cov, cargo-hack powerset, wasm32 + thumbv7em builds, `cargo bench --no-run`, run all examples, `cargo publish --dry-run`, concurrency cancel, weekly schedule

### Phase C: Usability & Automation
- [ ] Macro-generated typed input structs + typed `prove`/`verify`; name-keyed witness API; public-input layout printout; typed `KeygenOptions`
- [ ] Circuit registry (`inventory`/`linkme`) so the mismatch detector runs automatically (`cargo axiom check`)
- [ ] Real CLI as `cargo axiom`: `new`, `doctor`, `inspect circuit`, `check`, `keygen|prove|verify --in inputs.json`, `bench`, `explain`; clap, `--json`, completions
- [ ] Serialization: persist halo2 vk/params, versioned proof envelope with IR hash, `from_bytes`, serde on `ProofClaim`, `std::error::Error` + `Display` on all error types
- [ ] More macro ui tests + helpful suggestions (floats, `if`/`match`, `!=`, calls); validated `backend = "..."`

### Phase D: Missing Features & Innovation
- [ ] Correlated uncertainty (`CorrelatedFuzzy` / covariance vector with Jacobian propagation)
- [ ] Nonlinear ops on `Fuzzy` (`exp`, `ln`, `sqrt`, `powi`, `tanh`) with second-order mean correction; unscented-transform and Monte Carlo propagation modes
- [ ] More distributions (Uniform, Beta, Gamma, LogNormal, Student-t, Poisson, Binomial, mixtures); `pdf`/`cdf`/`quantile`; `prob_greater_than`; KL; `Sum`/`Product`; N-way fusion; conjugate updates; `from_logits`/`top_k`/`cross_entropy` in core
- [ ] Verifiable uncertain claims: prove a fused `Fuzzy` estimate/decision meets a threshold without revealing raw readings (fixed-point `Fuzzy` in-circuit)
- [ ] Proof-carrying `DecisionRecord`
- [ ] Gadgets: range proof, Poseidon, Merkle membership, bounded `for` unrolling, `!=`, `if`, division via witnessed quotient/remainder
- [ ] WASM browser playground (constraint/R1CS/halo2-row viewer; Fuzzy vs Monte Carlo demo)
- [ ] WASM / on-chain verifier (Groth16 BLS12-381; Solidity verifier generator stretch)

### Phase E: Examples, Templates & Docs
- [ ] Examples (each with README + expected output, run in CI): age/range proof (README lead), verifiable sensor-fusion claim, private credit-score threshold, ML decision with abstention, A/B test, separate prover/verifier services with proof files, custom-backend skeleton using the conformance suite
- [ ] Move examples into a workspace `examples/` crate
- [ ] `tpt-axiom-template` for `cargo generate` + `cargo axiom new --template age-proof|credit|sensor`
- [ ] Docs site (mdBook): tutorial, cookbook, when-to-use guide, comparison table, FAQ, honest-limits page, expanded `ARCHITECTURE.md` with "write a backend" guide
- [ ] README badges (crates.io, docs.rs, CI, MSRV, licence) and benchmark CPU/command details
