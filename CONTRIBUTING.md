# Contributing to tpt-axiom

Thanks for your interest in contributing! `tpt-axiom` is developed
phase-by-phase against [todo.md](todo.md); please check there (and
[spec.txt](spec.txt) / [ARCHITECTURE.md](ARCHITECTURE.md)) for the current
phase and design before starting work, to avoid overlapping with in-progress
efforts.

## Getting started

```sh
git clone https://github.com/tpt-solutions/tpt-axiom
cd tpt-axiom
cargo build --workspace
cargo test --workspace
```

## Development workflow

- Format code with `cargo fmt --all` before committing.
- Lint with `cargo clippy --workspace --all-targets -- -D warnings`.
- Run the full test suite with `cargo test --workspace`.
- All three checks run in CI (see `.github/workflows/ci.yml`) and must pass
  before a pull request can be merged.
- `unsafe_code` is forbidden workspace-wide (`#![forbid(unsafe_code)]`); new
  crates should keep this lint.
- Public items should be documented (`#![warn(missing_docs)]`); prefer
  documenting the *why* (invariants, formulas, non-obvious behavior) over
  restating the signature.

## Commit / PR guidelines

- Keep commits focused; prefer several small commits over one large one when
  a change has logically separate parts.
- Reference the relevant `todo.md` phase/item in your PR description.
- Add or update tests for any behavioral change. New probabilistic operations
  should include both closed-form unit tests and, where feasible, a Monte
  Carlo cross-check (see `tpt-axiom-core`'s test suite for examples).
- Update `CHANGELOG.md` under `[Unreleased]`.

## Code of conduct

Be respectful and constructive. Assume good faith, focus feedback on the
code and the design, and keep discussion technical.

## License

By contributing, you agree that your contributions will be dual-licensed
under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at the user's
option, matching the rest of the project.
