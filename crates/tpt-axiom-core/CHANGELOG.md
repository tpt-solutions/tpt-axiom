# Changelog

All notable changes to `tpt-axiom-core` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `intelligence` module: the probabilistic type system for AI/decision
  workloads — `Probability` (validated [0,1]), `Confidence` (semantically
  distinct, explicit conversion), `Bernoulli`, `Categorical<T>` (normalized,
  with entropy/sampling), `Uncertain<T>` (preserve-until-discarded),
  `Score<T>`, `Decision<T>` (first-class abstention + explicit policy
  conversion), `Evidence<T>` (multiplicative combination), and `Provenance`
  metadata. All re-exported through the `tpt-axiom` prelude.
- Opt-in `serde` feature serialising every intelligence type and `Fuzzy<T>` itself.
- Uncertainty Operations: `Probability::{noisy_or, conjunct, pooled, into_confidence}`,
  `Confidence::{conjunct, disjunct}`, `Evidence::combine_all`,
  `Categorical::bayesian_update`, `Distribution::{affine, map_first_order}`,
  `classify_by_confidence` + `Escalation`, `Reproducibility`, and the `Validate`
  deterministic-validation trait.
- Decision Types: `Ranking<T>`, `MultiLabelDecision`, `Hypotheses<T>`,
  `DecisionRecord<T>`, `Calibration`, `BinaryDecision`, `Categorical::decide`.
- Property-based invariant tests (proptest) and wire-format v1 serde
  roundtrip tests (`tests/properties.rs`).
- `families` module: `Uniform`, `Beta`, `Gamma`, `LogNormal`, `StudentT`,
  `Poisson`, `Binomial` behind the `ContinuousDistribution` /
  `DiscreteDistribution` traits — `mean`, `variance`, `pdf`/`pmf`, `cdf`,
  `quantile` (bracket-widening bisection), `prob_greater_than` — plus
  `Beta::update_bernoulli` / `Gamma::update_poisson` conjugate updates and
  `kl_gaussian`.
- `special` module: dependency-free `lgamma`/`gamma` (Lanczos g=7, n=9),
  `gamma_p`/`gamma_q` (series + Lentz continued fraction) and `beta_reg`
  (mirror form + continued fraction), accurate to ~1e-14 in their domains.
- `logits` module: temperature-scaled `softmax`/`log_softmax` (max-subtracted,
  so large logits do not overflow), `cross_entropy`, `top_k`.
- Nonlinear `Fuzzy` transforms: a generic `Fuzzy::transform(f, f', f'')` engine
  with second-order mean correction (`f(m) + f''(m)·v/2`) and derivative
  variance `(f'(m))²·v`, plus closed-form `exp`, `ln`, `sqrt`, `powi`, `tanh`
  that follow IEEE semantics out of domain instead of panicking.
- `Fuzzy::fuse_all` — checked N-way inverse-variance fusion.

### Changed

- Crate renamed from `axiom-core` to `tpt-axiom-core`.

### Fixed

- `distribution.rs` no longer fails to compile: added the missing `Constant`
  variant and imports that its own test suite already relied on.
- Removed `src/operator.rs` and `src/traits.rs`, dead duplicates of the
  arithmetic already implemented directly on `Fuzzy<T>` in `src/fuzzy.rs`
  (they were never wired into `lib.rs` and would have conflicted with it).
- Removed `src/monte_carlo_tests.rs`, a stale draft superseded by
  `tests/monte_carlo.rs`.

## [0.1.0] - Phase 1

### Added

- `Fuzzy<T>`: a mean + variance value with `Add`/`Sub`/`Mul`/`Div` operator
  overloads implementing first-order Gaussian error propagation, plus
  `fuse`, `confidence_interval`, `z_score`, and `one_sigma`.
- `Distribution<T>`: a general probabilistic value with `Gaussian` and
  `Constant` variants.
- `stats` module: dependency-free `erf`, `norm_cdf`, and `norm_ppf`
  approximations.
