//! Vendor-neutral interfaces for external AI and decision engines.
//!
//! An *external engine* is anything that turns an input into confidence
//! scores — an LLM judge, a cloud classifier, an ONNX model, a rules
//! engine. Axiom treats every such engine as an **untrusted score
//! producer** and owns the boundary:
//!
//! * **Validation at the boundary** — raw scores (probabilities or
//!   logits) are checked and normalized before they ever become Axiom
//!   types; NaN, out-of-range, and degenerate inputs fail loudly instead
//!   of smuggling invalid state past the caller.
//! * **Policies stay caller-owned** — nothing here decides for you.
//!   Every conversion to a [`Decision`] takes the threshold as an
//!   argument, and abstention is a first-class outcome.
//! * **Provenance travels with the output** — every engine result
//!   carries a [`Provenance`], so a downstream [`DecisionRecord`] can
//!   say which engine produced it and under what revision.
//! * **No vendor coupling** — [`DecisionEngine`] is the only seam an
//!   engine implements, it performs no I/O itself, and this crate gains
//!   no dependency on any engine, SDK, or network service. An adapter
//!   implements the trait against its vendor SDK; everything downstream
//!   of the trait stays vendor-neutral.
//!
//! Three output shapes cover the common wire formats:
//!
//! * [`EngineOutput<T>`] — one score per mutually exclusive outcome
//!   (softmax classification). Scores arrive either as probabilities
//!   (validated, then renormalized as relative weights) or as logits
//!   (converted through a numerically stable temperature softmax).
//! * [`EngineVerdict`] — a single binary score: one probability or one
//!   logit (sigmoid).
//! * [`MultiLabelOutput`] — one *independent* score per label (sigmoid
//!   heads); uncertainty never leaks between labels.
//!
//! Reading a chance as a decision confidence — as [`EngineOutput::decide`]
//! and [`threshold_decision`] do — is the same documented reinterpretation
//! [`Categorical::decide`] makes in `tpt-axiom-core`.
//!
//! # Examples
//!
//! A vendor adapter implements one trait and owns all the I/O:
//!
//! ```
//! use tpt_axiom_core::{Confidence, Provenance};
//! use tpt_axiom_interop::engine::{DecisionEngine, EngineError, EngineOutput};
//!
//! struct SentimentJudge; // would hold the vendor client, model id, …
//!
//! impl DecisionEngine for SentimentJudge {
//!     type Input = str;
//!     type Outcome = &'static str;
//!
//!     fn classify(&self, text: &str) -> Result<EngineOutput<&'static str>, EngineError> {
//!         // The vendor call goes here; these logits stand in for it.
//!         EngineOutput::from_logits(
//!             Provenance {
//!                 origin: String::from("sentiment-v2"),
//!                 revision: Some(String::from("1.4.0")),
//!                 ..Provenance::default()
//!             },
//!             [("positive", 1.2), ("negative", -0.4)],
//!             1.0,
//!         )
//!     }
//! }
//!
//! let judge = SentimentJudge;
//! let decision = judge.decide("great product", Confidence::new_or_panic(0.7))?;
//! assert_eq!(decision.committed(), Some(&"positive"));
//! # Ok::<(), EngineError>(())
//! ```

use tpt_axiom_core::{
    AbstentionReason, Bernoulli, Categorical, Confidence, Decision, DecisionRecord,
    InvalidProbability, MultiLabelDecision, Probability, Provenance, Ranking, Score,
};

/// A boundary rejection: the engine's raw output violated the domain its
/// math requires, or was structurally unusable.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    /// No outcomes (or scores) were supplied at all.
    NoOutcomes,
    /// A probability score violated its domain: not finite, or outside
    /// `[0, 1]`. The index is the score's position in the input order.
    InvalidScore {
        /// Position of the offending score in the input order.
        index: usize,
        /// The rejected score.
        score: f64,
    },
    /// A logit was not a finite number.
    InvalidLogit(f64),
    /// The softmax temperature was not finite and positive.
    InvalidTemperature(f64),
    /// Every score was zero, so no outcome can be preferred over another.
    Degenerate,
    /// The same label appeared twice in a multi-label output. Per-label
    /// independence means uncertainty must never leak between labels, and
    /// two scores for one label is exactly such a leak - the stricter
    /// (lower) one would silently win or lose depending on iteration order.
    DuplicateLabel(String),
    /// The binary threshold policy was degenerate: `active_at <= 0.5` makes
    /// the abstention band empty, so a coin-flip (`p == 0.5`) output would
    /// be *committed* rather than escalated. Require a strict majority.
    InvalidThreshold(f64),
    /// The core probability type rejected a value at construction.
    InvalidProbability(InvalidProbability),
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoOutcomes => f.write_str("engine output had no outcomes"),
            Self::InvalidScore { index, score } => {
                write!(
                    f,
                    "engine score at index {index} must be a finite probability in [0, 1], got {score}"
                )
            }
            Self::InvalidLogit(logit) => write!(f, "engine logit must be finite, got {logit}"),
            Self::InvalidTemperature(temperature) => {
                write!(
                    f,
                    "softmax temperature must be finite and positive, got {temperature}"
                )
            }
            Self::Degenerate => {
                f.write_str("every engine score was zero; no outcome can be preferred")
            }
            Self::DuplicateLabel(label) => {
                write!(f, "duplicate label `{label}` in multi-label output")
            }
            Self::InvalidThreshold(t) => write!(
                f,
                "binary threshold must exceed 0.5 (a coin flip must abstain, not commit), got {t}"
            ),
            Self::InvalidProbability(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// The caller-owned binary policy over the chance that something is
/// active.
///
/// Commit `true` when `active` — read as decision confidence — meets
/// `active_at`, commit `false` when its complement does, and abstain when
/// neither side is confident enough. This is the policy
/// [`EngineVerdict::decide`] and [`MultiLabelOutput::decide`] apply per
/// label; exposed because it is the reusable primitive for adapters with
/// their own binary heads.
///
/// `active_at` must exceed `0.5`: at or below a coin-flip level the
/// abstention band is empty (`p` or its complement always meets the
/// threshold), so a maximally uncertain `p == 0.5` output would be
/// *committed* instead of escalated - contradicting the abstention-first
/// design. The error names the rejected threshold.
///
/// # Errors
/// [`EngineError::InvalidThreshold`] when `active_at.value() <= 0.5`.
pub fn threshold_decision(
    active: Probability,
    active_at: Confidence,
) -> Result<Decision<bool>, EngineError> {
    if active_at.value() <= 0.5 {
        return Err(EngineError::InvalidThreshold(active_at.value()));
    }
    let p = active.into_confidence();
    if p.meets(active_at) {
        Ok(Decision::yes(p))
    } else {
        let not_p = active.complement().into_confidence();
        if not_p.meets(active_at) {
            Ok(Decision::no(not_p))
        } else {
            Ok(Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence,
            })
        }
    }
}

/// One normalized classification output from an external engine: a
/// validated, normalized distribution over mutually exclusive outcomes,
/// plus the provenance of the engine call that produced it.
///
/// Build through [`EngineOutput::from_probabilities`] (scores already in
/// `[0, 1]`) or [`EngineOutput::from_logits`] (raw logits, converted by a
/// numerically stable temperature softmax). Invalid inputs never become
/// an `EngineOutput`.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)] // f64-backed chances are not Eq
pub struct EngineOutput<T> {
    categorical: Categorical<T>,
    provenance: Provenance,
}

impl<T> EngineOutput<T> {
    /// Normalizes probability-style scores over mutually exclusive
    /// outcomes.
    ///
    /// Each score must be a finite probability in `[0, 1]`; the scores are
    /// then renormalized as *relative weights* (so a near-summing set from
    /// independent sigmoid heads still yields a valid distribution).
    ///
    /// # Errors
    /// [`EngineError::NoOutcomes`] on empty input,
    /// [`EngineError::InvalidScore`] for the first out-of-domain score,
    /// [`EngineError::Degenerate`] when every score is zero.
    pub fn from_probabilities(
        provenance: Provenance,
        scores: impl IntoIterator<Item = (T, f64)>,
    ) -> Result<Self, EngineError>
    where
        T: PartialEq,
    {
        let mut pairs = Vec::new();
        for (index, (outcome, score)) in scores.into_iter().enumerate() {
            match Probability::new(score) {
                Ok(p) => pairs.push((outcome, p.value())),
                Err(_) => return Err(EngineError::InvalidScore { index, score }),
            }
        }
        if pairs.is_empty() {
            return Err(EngineError::NoOutcomes);
        }
        let categorical = Categorical::new(pairs).map_err(|_| EngineError::Degenerate)?;
        Ok(Self {
            categorical,
            provenance,
        })
    }

    /// Normalizes raw logits over mutually exclusive outcomes through a
    /// numerically stable softmax at `temperature`.
    ///
    /// The max logit is subtracted before exponentiation, so arbitrarily
    /// large finite logits cannot overflow; `temperature` must be finite
    /// and positive — values below one sharpen the distribution, above
    /// one flatten it, and `1.0` is the plain softmax.
    ///
    /// # Errors
    /// [`EngineError::NoOutcomes`] on empty input,
    /// [`EngineError::InvalidLogit`] for a non-finite logit,
    /// [`EngineError::InvalidTemperature`] for a bad temperature.
    pub fn from_logits(
        provenance: Provenance,
        scores: impl IntoIterator<Item = (T, f64)>,
        temperature: f64,
    ) -> Result<Self, EngineError>
    where
        T: PartialEq,
    {
        let collected: Vec<(T, f64)> = scores.into_iter().collect();
        if collected.is_empty() {
            return Err(EngineError::NoOutcomes);
        }
        if !(temperature.is_finite() && temperature > 0.0) {
            return Err(EngineError::InvalidTemperature(temperature));
        }
        for (_, logit) in &collected {
            if !logit.is_finite() {
                return Err(EngineError::InvalidLogit(*logit));
            }
        }
        let max = collected
            .iter()
            .map(|(_, logit)| *logit)
            .fold(f64::NEG_INFINITY, f64::max);
        // Invariant: `logit - max <= 0` and `temperature > 0`, so every
        // exponent is non-positive and every weight lies in `[0, 1]` with
        // a total of at least one — the softmax cannot overflow.
        let weights = collected
            .into_iter()
            .map(|(outcome, logit)| (outcome, ((logit - max) / temperature).exp()))
            .collect::<Vec<_>>();
        let categorical = Categorical::new(weights).map_err(|_| EngineError::Degenerate)?;
        Ok(Self {
            categorical,
            provenance,
        })
    }

    /// Where this output came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The validated, normalized distribution over outcomes.
    #[must_use]
    pub const fn categorical(&self) -> &Categorical<T> {
        &self.categorical
    }

    /// Consumes the output into its distribution, dropping the
    /// provenance.
    #[must_use]
    pub fn into_categorical(self) -> Categorical<T> {
        self.categorical
    }

    /// Commits to the most likely outcome when its chance — read as
    /// decision confidence — meets `threshold`; abstains otherwise.
    /// The threshold is the caller's policy, never baked in here.
    #[must_use]
    pub fn decide(&self, threshold: Confidence) -> Decision<T>
    where
        T: Clone + PartialEq,
    {
        self.categorical.decide(threshold)
    }

    /// The best-first ranking over the outcome space, confidences read
    /// from the normalized chances.
    #[must_use]
    pub fn ranking(&self) -> Ranking<T>
    where
        T: Clone + PartialEq,
    {
        let candidates = self
            .categorical
            .outcomes()
            .iter()
            .map(|(outcome, p)| Score::new(outcome.clone(), p.into_confidence()))
            .collect();
        Ranking::new(candidates)
    }

    /// Bundles the decision under `threshold` with this output's
    /// provenance — the record a downstream audit consumes.
    #[must_use]
    pub fn into_record(self, threshold: Confidence) -> DecisionRecord<T>
    where
        T: Clone + PartialEq,
    {
        DecisionRecord::new(self.categorical.decide(threshold), self.provenance)
    }
}

/// A single binary score from an external engine: one probability (or one
/// logit, converted by the logistic function) plus provenance.
///
/// The binary counterpart of [`EngineOutput`], for engines that answer
/// one yes/no question per call.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)] // f64-backed chances are not Eq
pub struct EngineVerdict {
    probability: Probability,
    provenance: Provenance,
}

impl EngineVerdict {
    /// Wraps an engine's reported probability for the affirmative answer.
    ///
    /// # Errors
    /// [`EngineError::InvalidProbability`] when `p` is not a finite value
    /// in `[0, 1]`.
    pub fn from_probability(provenance: Provenance, p: f64) -> Result<Self, EngineError> {
        Ok(Self {
            probability: Probability::new(p).map_err(EngineError::InvalidProbability)?,
            provenance,
        })
    }

    /// Wraps an engine's reported logit, converted through the logistic
    /// function `p = 1 / (1 + exp(-logit))`.
    ///
    /// The conversion saturates gracefully: extreme finite logits yield
    /// probabilities arbitrarily close to (but never outside) `[0, 1]`.
    ///
    /// # Errors
    /// [`EngineError::InvalidLogit`] when the logit is not finite.
    pub fn from_logit(provenance: Provenance, logit: f64) -> Result<Self, EngineError> {
        if !logit.is_finite() {
            return Err(EngineError::InvalidLogit(logit));
        }
        let p = 1.0 / (1.0 + (-logit).exp());
        Ok(Self {
            probability: Probability::new(p).map_err(EngineError::InvalidProbability)?,
            provenance,
        })
    }

    /// The validated chance of the affirmative answer.
    #[must_use]
    pub const fn probability(&self) -> Probability {
        self.probability
    }

    /// Where this verdict came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The verdict as a two-outcome distribution.
    ///
    /// # Panics
    /// Never in practice: a validated [`Probability`] is exactly
    /// [`Bernoulli`]'s domain, so the constructor cannot fail.
    #[must_use]
    pub fn into_bernoulli(self) -> Bernoulli {
        // Invariant: a validated `Probability` is exactly `Bernoulli`'s
        // domain, so the constructor cannot fail.
        Bernoulli::new(self.probability.value())
            .expect("a validated probability is a valid Bernoulli parameter")
    }

    /// Applies [`threshold_decision`] with the caller's policy: commit
    /// yes when the affirmative chance meets `active_at`, commit no when
    /// the negative chance does, abstain in the uncertainty band.
    ///
    /// # Errors
    /// [`EngineError::InvalidThreshold`] when `active_at <= 0.5`.
    pub fn decide(&self, active_at: Confidence) -> Result<Decision<bool>, EngineError> {
        threshold_decision(self.probability, active_at)
    }
}

/// Independent per-label scores from an external engine — the "which of
/// these tags apply?" shape, where each label carries its own sigmoid
/// probability and uncertainty never leaks between labels.
///
/// Unlike [`EngineOutput`], the scores here are *not* normalized: `p = 0`
/// is a legitimate answer ("confidently not active"), and the set of
/// scores need not sum to anything in particular.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)] // f64-backed chances are not Eq
pub struct MultiLabelOutput {
    labels: Vec<(String, Probability)>,
    provenance: Provenance,
}

impl MultiLabelOutput {
    /// Validates one probability per label, in input order.
    ///
    /// # Errors
    /// [`EngineError::NoOutcomes`] on empty input,
    /// [`EngineError::InvalidScore`] for the first out-of-domain score,
    /// [`EngineError::DuplicateLabel`] when a label appears twice
    /// (independent per-label decisions cannot represent two scores for
    /// one label).
    pub fn from_probabilities(
        provenance: Provenance,
        scores: impl IntoIterator<Item = (impl Into<String>, f64)>,
    ) -> Result<Self, EngineError> {
        let mut labels = Vec::new();
        for (index, (label, score)) in scores.into_iter().enumerate() {
            match Probability::new(score) {
                Ok(p) => {
                    let label = label.into();
                    if labels.iter().any(|(known, _)| *known == label) {
                        return Err(EngineError::DuplicateLabel(label));
                    }
                    labels.push((label, p));
                }
                Err(_) => return Err(EngineError::InvalidScore { index, score }),
            }
        }
        if labels.is_empty() {
            return Err(EngineError::NoOutcomes);
        }
        Ok(Self { labels, provenance })
    }

    /// Where this output came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The validated per-label chances, in input order (duplicates were
    /// rejected at construction).
    #[must_use]
    pub fn labels(&self) -> &[(String, Probability)] {
        &self.labels
    }

    /// The validated chance for `label`, if the engine reported it.
    #[must_use]
    pub fn probability_of(&self, label: &str) -> Option<Probability> {
        self.labels
            .iter()
            .find(|(known, _)| known == label)
            .map(|(_, p)| *p)
    }

    /// Applies [`threshold_decision`] per label under the caller's
    /// policy: each label independently commits active, commits inactive,
    /// or abstains.
    ///
    /// # Errors
    /// [`EngineError::InvalidThreshold`] when `active_at <= 0.5`.
    pub fn decide(&self, active_at: Confidence) -> Result<MultiLabelDecision, EngineError> {
        let mut decision = MultiLabelDecision::new();
        for (label, p) in &self.labels {
            decision = decision.with(label.as_str(), threshold_decision(*p, active_at)?);
        }
        Ok(decision)
    }
}

/// The seam an external AI or decision engine implements: map an input to
/// one normalized [`EngineOutput`].
///
/// The trait performs no I/O and takes no stance on transport, model
/// format, or vendor SDK — the implementor owns all of that. Everything
/// downstream of the trait (validation, normalization, policy, audit)
/// is vendor-neutral, so an engine registry can hold
/// `Box<dyn DecisionEngine<Input = ..., Outcome = ...>>` and mix vendors
/// freely.
///
/// The provided methods are convenience compositions over [`classify`]:
/// [`decide`] applies the caller's threshold policy, [`rank`] builds the
/// best-first ranking.
///
/// [`classify`]: Self::classify
/// [`decide`]: Self::decide
/// [`rank`]: Self::rank
pub trait DecisionEngine {
    /// The input the engine judges (e.g. `str`, `&[f64]`, a request type).
    /// `?Sized` so borrowed slice/str inputs work without cloning.
    type Input: ?Sized;
    /// The outcome space the engine chooses over.
    type Outcome: Clone + PartialEq;

    /// Runs the vendor call and normalizes its raw scores at the
    /// boundary.
    ///
    /// # Errors
    /// Whatever the engine call fails with, plus every
    /// [`EngineError`] the boundary validation can raise.
    fn classify(&self, input: &Self::Input) -> Result<EngineOutput<Self::Outcome>, EngineError>;

    /// Commits to the most likely outcome when its chance — read as
    /// decision confidence — meets `threshold`; abstains otherwise.
    ///
    /// # Errors
    /// Propagates [`classify`](Self::classify).
    fn decide(
        &self,
        input: &Self::Input,
        threshold: Confidence,
    ) -> Result<Decision<Self::Outcome>, EngineError> {
        Ok(self.classify(input)?.decide(threshold))
    }

    /// The best-first ranking over the outcome space.
    ///
    /// # Errors
    /// Propagates [`classify`](Self::classify).
    fn rank(&self, input: &Self::Input) -> Result<Ranking<Self::Outcome>, EngineError> {
        Ok(self.classify(input)?.ranking())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on analytic results

    use super::*;

    fn provenance() -> Provenance {
        Provenance {
            origin: String::from("test-engine"),
            ..Provenance::default()
        }
    }

    #[test]
    fn probability_scores_are_relative_weights() {
        let output =
            EngineOutput::from_probabilities(provenance(), [("spam", 0.6), ("ham", 0.3)]).unwrap();
        // Renormalized: 0.6 / 0.9 and 0.3 / 0.9.
        let spam = output.categorical().probability_of(&"spam").value();
        let ham = output.categorical().probability_of(&"ham").value();
        assert!((spam - 2.0 / 3.0).abs() < 1e-12);
        assert!((ham - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(output.provenance().origin, "test-engine");
    }

    #[test]
    fn already_normalized_scores_survive() {
        let output =
            EngineOutput::from_probabilities(provenance(), [("a", 0.25), ("b", 0.75)]).unwrap();
        assert_eq!(output.categorical().probability_of(&"a").value(), 0.25);
        assert_eq!(output.categorical().probability_of(&"b").value(), 0.75);
    }

    #[test]
    fn probability_scores_are_validated() {
        // NaN never equals itself, so match structurally:
        assert!(matches!(
            EngineOutput::from_probabilities(provenance(), [("a", f64::NAN)]),
            Err(EngineError::InvalidScore { index: 0, .. })
        ));
        assert_eq!(
            EngineOutput::from_probabilities(provenance(), [("a", 0.5), ("b", 1.5)]),
            Err(EngineError::InvalidScore {
                index: 1,
                score: 1.5
            })
        );
        let empty: [(&str, f64); 0] = [];
        assert_eq!(
            EngineOutput::from_probabilities(provenance(), empty),
            Err(EngineError::NoOutcomes)
        );
        assert_eq!(
            EngineOutput::from_probabilities(provenance(), [("a", 0.0), ("b", 0.0)]),
            Err(EngineError::Degenerate)
        );
    }

    #[test]
    fn logits_softmax_matches_closed_form() {
        // exp(ln 2) = 2: 2 / 3 and 1 / 3.
        let output =
            EngineOutput::from_logits(provenance(), [("a", 2.0_f64.ln()), ("b", 0.0)], 1.0)
                .unwrap();
        let a = output.categorical().probability_of(&"a").value();
        let b = output.categorical().probability_of(&"b").value();
        assert!((a - 2.0 / 3.0).abs() < 1e-12);
        assert!((b - 1.0 / 3.0).abs() < 1e-12);

        // Logits [1, -1]: p = e^2 / (e^2 + 1) at temperature 1...
        let sharp = EngineOutput::from_logits(provenance(), [("a", 1.0), ("b", -1.0)], 1.0)
            .unwrap()
            .categorical()
            .probability_of(&"a")
            .value();
        let e_squared = 2.0_f64.exp();
        assert!((sharp - e_squared / (e_squared + 1.0)).abs() < 1e-12);
        // ...and e / (e + 1) at temperature 2 (flatter).
        let flat = EngineOutput::from_logits(provenance(), [("a", 1.0), ("b", -1.0)], 2.0)
            .unwrap()
            .categorical()
            .probability_of(&"a")
            .value();
        assert!((flat - (core::f64::consts::E / (core::f64::consts::E + 1.0))).abs() < 1e-12);
        assert!(flat < sharp);
    }

    #[test]
    fn extreme_logits_do_not_overflow() {
        let output =
            EngineOutput::from_logits(provenance(), [("a", 1.0e300), ("b", 0.0)], 1.0).unwrap();
        assert_eq!(output.categorical().probability_of(&"a").value(), 1.0);
        assert_eq!(output.categorical().probability_of(&"b").value(), 0.0);
    }

    #[test]
    fn logits_are_validated() {
        assert_eq!(
            EngineOutput::from_logits(provenance(), [("a", f64::INFINITY)], 1.0),
            Err(EngineError::InvalidLogit(f64::INFINITY))
        );
        assert_eq!(
            EngineOutput::from_logits(provenance(), [("a", 0.0)], 0.0),
            Err(EngineError::InvalidTemperature(0.0))
        );
        assert_eq!(
            EngineOutput::from_logits(provenance(), [("a", 0.0)], -1.0),
            Err(EngineError::InvalidTemperature(-1.0))
        );
        let empty: [(&str, f64); 0] = [];
        assert_eq!(
            EngineOutput::from_logits(provenance(), empty, 1.0),
            Err(EngineError::NoOutcomes)
        );
    }

    #[test]
    fn output_policy_commits_or_abstains() {
        // Exactly normalized: best outcome's chance is 0.75.
        let output =
            EngineOutput::from_probabilities(provenance(), [("a", 0.75), ("b", 0.25)]).unwrap();
        assert_eq!(
            output.decide(Confidence::new_or_panic(0.7)),
            Decision::Commit {
                value: "a",
                confidence: Confidence::new_or_panic(0.75)
            }
        );
        assert_eq!(
            output.decide(Confidence::new_or_panic(0.8)),
            Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence
            }
        );
    }

    #[test]
    fn output_ranks_and_records() {
        let output =
            EngineOutput::from_probabilities(provenance(), [("a", 0.25), ("b", 0.75)]).unwrap();
        let ranking = output.ranking();
        assert_eq!(ranking.best().unwrap().value(), &"b");

        let record = output.into_record(Confidence::new_or_panic(0.5));
        assert_eq!(record.decision().committed(), Some(&"b"));
        assert_eq!(record.provenance().origin, "test-engine");
    }

    #[test]
    fn verdict_conversions_are_exact() {
        // sigmoid(ln 3) = 3/4.
        let verdict = EngineVerdict::from_logit(provenance(), 3.0_f64.ln()).unwrap();
        assert!((verdict.probability().value() - 0.75).abs() < 1e-12);
        assert!((verdict.into_bernoulli().p().value() - 0.75).abs() < 1e-12);

        let direct = EngineVerdict::from_probability(provenance(), 0.75).unwrap();
        assert_eq!(direct.probability().value(), 0.75);

        assert_eq!(
            EngineVerdict::from_probability(provenance(), 1.5),
            Err(EngineError::InvalidProbability(
                tpt_axiom_core::InvalidProbability(1.5)
            ))
        );
        assert!(matches!(
            EngineVerdict::from_logit(provenance(), f64::NAN),
            Err(EngineError::InvalidLogit(_))
        ));
    }

    #[test]
    fn extreme_logits_saturate_in_domain() {
        let high = EngineVerdict::from_logit(provenance(), 1.0e300).unwrap();
        assert_eq!(high.probability().value(), 1.0);
        let low = EngineVerdict::from_logit(provenance(), -1.0e300).unwrap();
        assert_eq!(low.probability().value(), 0.0);
    }

    #[test]
    fn verdict_policy_has_three_bands() {
        let confident = EngineVerdict::from_probability(provenance(), 0.9).unwrap();
        assert_eq!(
            confident.decide(Confidence::new_or_panic(0.8)).unwrap(),
            Decision::Commit {
                value: true,
                confidence: Confidence::new_or_panic(0.9)
            }
        );
        // p = 0.2 is a confident *no* at the same threshold.
        let negative = EngineVerdict::from_probability(provenance(), 0.2).unwrap();
        assert_eq!(
            negative.decide(Confidence::new_or_panic(0.8)).unwrap(),
            Decision::Commit {
                value: false,
                confidence: Confidence::new_or_panic(0.8)
            }
        );
        // p = 0.5 falls in the uncertainty band either way.
        let uncertain = EngineVerdict::from_probability(provenance(), 0.5).unwrap();
        assert_eq!(
            uncertain.decide(Confidence::new_or_panic(0.8)).unwrap(),
            Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence
            }
        );
    }

    #[test]
    fn coin_flip_thresholds_are_rejected() {
        // active_at <= 0.5 empties the abstention band: p = 0.5 would
        // *commit* instead of escalate. The policy refuses to run.
        let coin = EngineVerdict::from_probability(provenance(), 0.5).unwrap();
        for bad in [Confidence::NONE, Confidence::new_or_panic(0.5)] {
            assert_eq!(
                coin.decide(bad),
                Err(EngineError::InvalidThreshold(bad.value()))
            );
        }
        // A strict-majority threshold is the minimum viable policy.
        assert!(coin.decide(Confidence::new_or_panic(0.5 + f64::EPSILON)).is_ok());
    }

    #[test]
    fn multi_label_decides_per_label() {
        let output = MultiLabelOutput::from_probabilities(
            provenance(),
            [("toxic", 0.9), ("spam", 0.5), ("rant", 0.2)],
        )
        .unwrap();
        assert_eq!(output.probability_of("toxic").unwrap().value(), 0.9);
        assert_eq!(output.probability_of("missing"), None);

        let decision = output
            .decide(Confidence::new_or_panic(0.75))
            .expect("valid threshold");
        assert_eq!(decision.value_of("toxic"), Some(true));
        assert_eq!(decision.value_of("spam"), None); // uncertainty band
        assert_eq!(decision.value_of("rant"), Some(false)); // 1 - 0.2 >= 0.75
        assert_eq!(decision.active_labels(), vec!["toxic"]);
    }

    #[test]
    fn duplicate_labels_are_rejected() {
        assert_eq!(
            MultiLabelOutput::from_probabilities(
                provenance(),
                [("spam", 0.9), ("toxic", 0.3), ("spam", 0.1)],
            ),
            Err(EngineError::DuplicateLabel(String::from("spam")))
        );
    }

    #[test]
    fn multi_label_validates_and_requires_labels() {
        assert_eq!(
            MultiLabelOutput::from_probabilities(provenance(), [("a", -0.1)]),
            Err(EngineError::InvalidScore {
                index: 0,
                score: -0.1
            })
        );
        let empty: [(&str, f64); 0] = [];
        assert_eq!(
            MultiLabelOutput::from_probabilities(provenance(), empty),
            Err(EngineError::NoOutcomes)
        );
        // All-zero scores are legitimate here: every label is confidently
        // inactive, unlike the normalized single-head case.
        let zeros = MultiLabelOutput::from_probabilities(provenance(), [("a", 0.0)]).unwrap();
        assert_eq!(
            zeros
                .decide(Confidence::new_or_panic(0.75))
                .expect("strict-majority threshold")
                .value_of("a"),
            Some(false)
        );
    }

    #[test]
    fn engine_trait_defaults_compose() {
        struct AlwaysUniform;

        impl DecisionEngine for AlwaysUniform {
            type Input = str;
            type Outcome = &'static str;

            fn classify(&self, _input: &str) -> Result<EngineOutput<&'static str>, EngineError> {
                EngineOutput::from_probabilities(provenance(), [("yes", 0.5), ("no", 0.5)])
            }
        }

        let engine = AlwaysUniform;
        let output = engine.classify("anything").unwrap();
        assert_eq!(output.categorical().probability_of(&"yes").value(), 0.5);
        assert_eq!(
            engine
                .decide("anything", Confidence::new_or_panic(0.6))
                .unwrap(),
            Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence
            }
        );
        let ranking = engine.rank("anything").unwrap();
        assert_eq!(ranking.candidates().len(), 2);
    }

    #[test]
    fn engine_trait_errors_propagate() {
        struct BrokenEngine;

        impl DecisionEngine for BrokenEngine {
            type Input = str;
            type Outcome = &'static str;

            fn classify(&self, _input: &str) -> Result<EngineOutput<&'static str>, EngineError> {
                Err(EngineError::NoOutcomes)
            }
        }

        assert_eq!(
            BrokenEngine.decide("anything", Confidence::FULL),
            Err(EngineError::NoOutcomes)
        );
        assert_eq!(BrokenEngine.rank("anything"), Err(EngineError::NoOutcomes));
    }

    #[test]
    fn errors_are_displayable() {
        let cases = [
            EngineError::NoOutcomes.to_string(),
            EngineError::InvalidScore {
                index: 2,
                score: 7.0,
            }
            .to_string(),
            EngineError::InvalidLogit(f64::NAN).to_string(),
            EngineError::InvalidTemperature(0.0).to_string(),
            EngineError::Degenerate.to_string(),
        ];
        assert!(cases.iter().all(|s| !s.is_empty()));
        assert!(cases[1].contains("index 2"));
    }
}
