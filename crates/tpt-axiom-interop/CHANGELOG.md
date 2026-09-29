# Changelog

All notable changes to `tpt-axiom-interop` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `augur` module: validated conversions between `tpt_augur_std::Dist` and
  Axiom's `Fuzzy`/`Distribution`/`Uncertain`/`Bernoulli` (exact closed-form
  moment matching; `Normal` is information-preserving and round-trips).
- `inference` module: `InferenceSample` — the runtime-agnostic boundary
  contract for the TPT inference runtimes (sampled value + log-space weight
  to `Evidence`/`Score`), awaiting their crates.io publication for the
  feature-gated `From` impls.
