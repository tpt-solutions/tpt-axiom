# Probabilistic decisions

The intelligence types model decisions rather than measurements:

* [`Probability`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/struct.Probability.html)
  is the *chance of an outcome*;
  [`Confidence`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/struct.Confidence.html)
  is the *belief in a result*. Same range, different types — conversion is
  explicit and visible at the call site.
* [`Decision<T>`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/enum.Decision.html)
  makes abstention a first-class outcome: `Decision::Abstain { reason }`
  instead of an ad-hoc `None`. Every policy threshold is a caller argument.
* Evidence combines in log space (no overflow), Bayesian updates run on
  `Categorical`/`Hypotheses`, and `classify_by_confidence` gives the
  three-way accept/review/reject escalation primitive.
* Rankings, multi-label decisions (per-label, uncertainty never leaks
  between labels), decision records with provenance, and calibration
  metadata round out the vocabulary.

The engine boundary in `tpt-axiom-interop` turns any external model into
these types: validated scores, provenance attached, duplicate labels and
coin-flip thresholds rejected at the boundary. The `ml_decision_abstention`
example shows the whole chain ending in an honest abstention.
