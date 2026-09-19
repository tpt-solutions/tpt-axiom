# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
