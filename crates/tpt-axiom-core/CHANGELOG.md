# Changelog

All notable changes to `tpt-axiom-core` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
