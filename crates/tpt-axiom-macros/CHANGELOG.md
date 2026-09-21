# Changelog

All notable changes to `tpt-axiom-macros` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Crate renamed from `axiom-macros` to `tpt-axiom-macros`.
- Crate-resolution lookups in `resolve_crate` updated for the renamed
  `tpt-axiom-ir` / `tpt-axiom-zk` package names.

### Fixed

- `lower_function` no longer fails to compile under Rust 2024: the local
  variable named `gen` (now a reserved keyword) was renamed to `lowerer`.

## [0.1.0] - Phase 2

### Added

- `#[zk_provable(backend = "...")]` attribute macro lowering straight-line
  arithmetic/assert function bodies into a generated `CircuitDefinition`.
- Support for `#[public]`/`#[secret]` parameter annotations, comparison
  assertions, `assert_eq!`, and single-expression public outputs.
