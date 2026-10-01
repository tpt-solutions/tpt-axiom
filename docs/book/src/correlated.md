# Correlated uncertainty

`Fuzzy` assumes independence.
[`Correlated`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/struct.Correlated.html)
drops that assumption: a mean vector plus a full covariance matrix,
validated (finite, symmetric, positive semi-definite via Cholesky) at
construction.

Propagation is exact linear algebra:

* `weighted_sum(&[1.0, -1.0])` computes `wᵀΣw` — with correlation +1 and
  equal variances, the difference `x − x` collapses to *certainty*, where
  independent `Fuzzy` arithmetic would report `2v`;
* `linear_map(&a)` propagates `AΣAᵀ` through the whole matrix.

Nonlinear correlated maps (Jacobians per transform) are future work; the
type covers the linear-exactness gap the `Fuzzy` docs warn about.
