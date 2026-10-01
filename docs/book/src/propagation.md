# Propagation modes

When a transform is too nonlinear for the delta method, two alternatives
live in [`tpt_axiom_core::propagate`](https://docs.rs/tpt-axiom-core):

* **Monte Carlo** (`monte_carlo`): draws `(x, y)` pairs from the two
  Gaussian models, pushes them through `f`, and reports
  Welford-accumulated sample moments. Convergence is the usual `1/√N`; the
  RNG is an injected `FnMut() -> f64` uniform generator, so the core stays
  dependency-free and fixed seeds make tests reproducible.
* **Unscented transform** (`unscented`): five deterministic sigma points
  (mean and `±√3σ` per axis, standard weights), weighted moments recovered
  in closed form. Five evaluations of `f`, no RNG, and second-order
  accuracy in the mean — for `exp`, the UT mean lands near the exact
  lognormal `e^{m+v/2}` where the delta method reports `e^m`.

Both assume independent inputs, exactly like the operators. Both are also
cross-checked in the test suite against the closed forms: exact for linear
maps, and within statistical/second-order tolerance of the lognormal mean
for `exp`.
