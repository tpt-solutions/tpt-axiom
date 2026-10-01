# Contributing to tpt-axiom

Thanks for your interest in `tpt-axiom`! Contributions are **issues only**:
pull requests are not accepted and will be closed without review.

## How to contribute

Please [open an issue](https://github.com/tpt-solutions/tpt-axiom/issues) for:

- **Bug reports**: include what you ran, what you expected, what happened,
  and your Rust version and OS. A minimal reproduction helps a lot.
- **Feature requests and ideas**: describe the use case, not just the
  solution.
- **Questions and documentation gaps**: if something was unclear, that is a
  bug in the docs.

Before filing, check [todo.md](todo.md), [ARCHITECTURE.md](ARCHITECTURE.md) and
[docs/design/spec.md](docs/design/spec.md), and search existing issues to avoid
duplicates.

## Reproducing locally

```sh
git clone https://github.com/tpt-solutions/tpt-axiom
cd tpt-axiom
cargo test --workspace --all-features
```

`--all-features` matters: proving backends are feature-gated, so a
default-feature build compiles out the end-to-end `prove`/`keygen` tests
instead of running them.

## Code of conduct

Be respectful and constructive. Assume good faith, focus feedback on the
code and the design, and keep discussion technical.
