# Distribution families

[`Distribution<T>`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/enum.Distribution.html)
is the Gaussian-shaped summary the arithmetic operates on. The *inputs* to a
pipeline usually have more shape — that is
[`tpt_axiom_core::families`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/families/index.html):

* `Uniform`, `Beta`, `Gamma` (shape/rate), `LogNormal`, `StudentT`,
  `Poisson`, `Binomial`;
* every family has validated construction, exact moments, `pdf`, `cdf`,
  `quantile`, and `prob_greater_than` (computed through the crate's own
  special functions — Lanczos lgamma, incomplete gamma/beta — accurate to
  ~1e-13 with no external dependencies);
* the conjugate updates are one call each: `Beta::update_bernoulli` and
  `Gamma::update_poisson`;
* `credible_interval(level)` on the continuous trait gives the central
  interval;
* conversions into the uncertainty arithmetic go through `.moments()` —
  only the first two moments survive, which is the same honesty rule the
  Augur bridge's `_approx` conversions carry.

Extras in the same module: exact Gaussian KL divergence, and
`tpt_axiom_core::logits` (`softmax`, `cross_entropy`, `top_k`) for model
outputs. The `ab_test` example in the repository puts the pieces together:
posteriors, credible intervals, `P(B > A)` by quadrature, and an explicit
ship policy.
