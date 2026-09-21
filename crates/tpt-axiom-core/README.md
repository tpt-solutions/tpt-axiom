# tpt-axiom-core

Probabilistic and uncertainty types for [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) — `no_std`, dependency-light, and usable on its own.

This crate provides [`Fuzzy<T>`], a value paired with a variance that propagates
automatically through ordinary arithmetic, and [`Distribution<T>`], a more
general probabilistic value (currently Gaussian and constant cases).

## `Fuzzy<T>`

Every `+`, `-`, `*`, `/` on a `Fuzzy<T>` produces a new `Fuzzy<T>` whose mean
and variance are derived from the first-order (Gaussian) error-propagation
rules — the same formulas you'd use by hand for sensor fusion, Kalman filters,
or quantitative error analysis, but expressed as ordinary Rust arithmetic:

```rust
use tpt_axiom_core::Fuzzy;

let pos: Fuzzy<f64> = Fuzzy::new(10.5, 0.5); // mean 10.5, variance 0.5
let vel: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
let dt = 0.2_f64;

let next_pos = pos + vel * dt; // uncertainty propagates automatically
assert!((next_pos.mean() - 10.9).abs() < 1e-9);
```

`Fuzzy<T>` also supports `fuse` (minimum-variance combination of two
independent estimates, i.e. a one-shot Kalman update), `confidence_interval`,
`z_score`, and `one_sigma`.

## `Distribution<T>`

`Distribution<T>` is a coarser-grained probabilistic wrapper with `Gaussian`
and `Constant` variants, arithmetic operators that delegate to `Fuzzy<T>`'s
propagation rules, and a `sample` method for Monte Carlo use.

## Feature status

Implemented (Phase 1): `Fuzzy<T>` arithmetic and statistics, `Distribution<T>`,
and the `stats` module (`erf`, `norm_cdf`, `norm_ppf`).

**Known limitation:** the `erf`/`norm_ppf` approximations in `stats` trade
some precision for a small, dependency-free implementation (~1e-6 relative
error in the tails). `Fuzzy::confidence_interval` and `Distribution::sample`
inherit that error bound — don't rely on them for sub-`1e-6` precision.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
