//! Typed decision representations for AI outputs: rankings, multi-label
//! sets, competing hypotheses, decision records, and calibration metadata.
//!
//! Everything here composes with [`Decision`](crate::Decision) and
//! [`Score`](crate::Score) rather than replacing them — a ranking is a
//! best-first sequence of scores, a multi-label decision is a set of
//! per-label binary decisions, and competing hypotheses are a normalized
//! [`Categorical`](crate::Categorical) with hypothesis-shaped accessors.
//! See the `intelligence` module for the underlying value types.

use alloc::string::String;
use alloc::vec::Vec;

use crate::intelligence::{
    AbstentionReason, Categorical, Confidence, Decision, Probability, Provenance, Score,
};

/// A best-first sequence of scored candidates: the ranking/selection
/// representation.
///
/// Construction sorts by confidence (descending, stable), so `best` is the
/// highest-confidence candidate and the order is total unless two candidates
/// tie.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranking<T> {
    candidates: Vec<Score<T>>,
}

impl<T> Ranking<T> {
    /// Builds a ranking from unsorted candidates, ordering by confidence.
    #[must_use]
    pub fn new(mut candidates: Vec<Score<T>>) -> Self {
        candidates.sort_by(|a, b| b.confidence().value().total_cmp(&a.confidence().value()));
        Self { candidates }
    }

    /// The highest-confidence candidate, or `None` when empty.
    #[must_use]
    pub fn best(&self) -> Option<&Score<T>> {
        self.candidates.first()
    }

    /// The ranked candidates, best first.
    #[must_use]
    pub fn candidates(&self) -> &[Score<T>] {
        &self.candidates
    }

    /// Applies the same explicit policy [`Decision::from_score`] uses, but to
    /// the best candidate of the ranking.
    ///
    /// An empty ranking abstains with [`AbstentionReason::NoCandidates`].
    #[must_use]
    pub fn decide(&self, threshold: Confidence) -> Decision<T>
    where
        T: Clone,
    {
        self.best().map_or_else(
            || Decision::Abstain {
                reason: AbstentionReason::NoCandidates,
            },
            |score| {
                let (value, confidence) = score.clone().into_parts();
                if confidence.meets(threshold) {
                    Decision::Commit { value, confidence }
                } else {
                    Decision::Abstain {
                        reason: AbstentionReason::InsufficientConfidence,
                    }
                }
            },
        )
    }
}

/// A multi-label decision: one binary decision per label.
///
/// This is the representation for "which of these tags apply?" — each label
/// independently commits or abstains, and uncertainty never leaks between
/// labels.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MultiLabelDecision {
    labels: Vec<(String, Decision<bool>)>,
}

impl MultiLabelDecision {
    /// An empty multi-label decision.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) the decision for `label`.
    #[must_use]
    pub fn with(mut self, label: impl Into<String>, decision: Decision<bool>) -> Self {
        let label = label.into();
        if let Some(slot) = self.labels.iter_mut().find(|(known, _)| *known == label) {
            slot.1 = decision;
        } else {
            self.labels.push((label, decision));
        }
        self
    }

    /// Convenience for the common "commit this label on/off with confidence"
    /// case.
    #[must_use]
    pub fn with_label(
        mut self,
        label: impl Into<String>,
        active: bool,
        confidence: Confidence,
    ) -> Self {
        self = self.with(
            label,
            Decision::Commit {
                value: active,
                confidence,
            },
        );
        self
    }

    /// The committed boolean for `label`, or `None` if it abstained or is
    /// absent.
    #[must_use]
    pub fn value_of(&self, label: &str) -> Option<bool> {
        self.labels
            .iter()
            .find(|(known, _)| known == label)
            .and_then(|(_, d)| d.committed().copied())
    }

    /// All labels with their decisions, in insertion order.
    #[must_use]
    pub fn labels(&self) -> &[(String, Decision<bool>)] {
        &self.labels
    }

    /// Labels whose decision committed to `true`.
    #[must_use]
    pub fn active_labels(&self) -> Vec<&str> {
        self.labels
            .iter()
            .filter(|(_, d)| d.committed() == Some(&true))
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

/// Competing hypotheses: a normalized [`Categorical`] read through a
/// hypothesis lens, with Bayesian revision built in.
#[derive(Clone, Debug, PartialEq)]
pub struct Hypotheses<T> {
    posterior: Categorical<T>,
}

impl<T: PartialEq> Hypotheses<T> {
    /// Wraps a normalized categorical as the current posterior.
    ///
    /// # Errors
    /// Propagates [`crate::EmptyCategorical`] for an empty categorical.
    pub fn from_prior(prior: Categorical<T>) -> Result<Self, crate::EmptyCategorical> {
        if prior.outcomes().is_empty() {
            return Err(crate::EmptyCategorical);
        }
        Ok(Self { posterior: prior })
    }

    /// Revises the posterior with a likelihood function over hypotheses.
    ///
    /// # Errors
    /// Propagates [`crate::EmptyCategorical`] if the revised weights are all
    /// non-positive.
    pub fn revise(&self, likelihood: impl Fn(&T) -> f64) -> Result<Self, crate::EmptyCategorical>
    where
        T: Clone,
    {
        let revised: Vec<(T, f64)> = self
            .posterior
            .outcomes()
            .iter()
            .map(|(h, p)| ((*h).clone(), p.value() * likelihood(h)))
            .collect();
        Ok(Self {
            posterior: Categorical::new(revised)?,
        })
    }

    /// The current posterior over hypotheses.
    #[must_use]
    pub const fn posterior(&self) -> &Categorical<T> {
        &self.posterior
    }

    /// The current best hypothesis and its posterior chance.
    #[must_use]
    pub fn leader(&self) -> (&T, Probability) {
        self.posterior.most_likely()
    }
}

/// A decision bundled with the provenance of how it was reached — the
/// decision-provenance/backend-metadata representation.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionRecord<T> {
    decision: Decision<T>,
    provenance: Provenance,
}

impl<T> DecisionRecord<T> {
    /// Records a decision with its provenance.
    #[must_use]
    pub const fn new(decision: Decision<T>, provenance: Provenance) -> Self {
        Self {
            decision,
            provenance,
        }
    }

    /// The decision.
    #[must_use]
    pub const fn decision(&self) -> &Decision<T> {
        &self.decision
    }

    /// Where the decision came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// Calibration metadata: how the confidences a producer emits relate to
/// observed outcomes, when that relationship has been measured.
///
/// Purely descriptive and vendor-neutral: `method` names the calibration
/// procedure ("temperature scaling", "isotonic", " Platt"), `reference` the
/// evaluation set or process that measured it, and `expected_calibration_error`
/// the summary statistic when available.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Calibration {
    /// Calibration procedure applied by the producer, if any.
    pub method: Option<String>,
    /// What the calibration was measured against.
    pub reference: Option<String>,
    /// Expected calibration error in `[0, 1]`, when measured.
    pub expected_calibration_error: Option<f64>,
}

impl Calibration {
    /// Builds a validated [`Calibration`]: the checked-constructor form of
    /// the raw struct literal (whose public fields bypass validation and
    /// remain for compatibility - prefer this when constructing).
    ///
    /// # Errors
    /// [`crate::InvalidProbability`] when `expected_calibration_error` is
    /// outside `[0, 1]` or NaN.
    pub fn new(
        method: Option<String>,
        reference: Option<String>,
        expected_calibration_error: Option<f64>,
    ) -> Result<Self, crate::InvalidProbability> {
        if let Some(ece) = expected_calibration_error {
            crate::Probability::new(ece)?;
        }
        Ok(Self {
            method,
            reference,
            expected_calibration_error,
        })
    }

    /// Validates the summary statistic, when present.
    ///
    /// # Errors
    /// An invalid ECE value (outside `[0, 1]` or NaN).
    pub fn validate(&self) -> Result<(), crate::InvalidProbability> {
        self.expected_calibration_error
            .map_or(Ok(()), |ece| crate::Probability::new(ece).map(|_| ()))
    }
}

/// A binary decision with its confidence — the yes/no representation.
///
/// This is [`Decision<bool>`] with constructors that read like the question
/// being answered.
pub type BinaryDecision = Decision<bool>;

/// Constructors for binary decisions.
impl Decision<bool> {
    /// Commits to `true` (yes) with the given confidence.
    #[must_use]
    pub const fn yes(confidence: Confidence) -> Self {
        Self::Commit {
            value: true,
            confidence,
        }
    }

    /// Commits to `false` (no) with the given confidence.
    #[must_use]
    pub const fn no(confidence: Confidence) -> Self {
        Self::Commit {
            value: false,
            confidence,
        }
    }
}

/// A categorical choice: the most likely outcome of a distribution, committed
/// through an explicit policy.
impl<T: PartialEq> Categorical<T> {
    /// Commits to the most likely outcome when its chance, read as a
    /// confidence, meets `threshold`; otherwise abstains.
    ///
    /// The `Probability -> Confidence` reinterpretation is deliberate and
    /// documented here: the chance of the chosen outcome is being used as the
    /// decision's stated confidence.
    #[must_use]
    pub fn decide(&self, threshold: Confidence) -> Decision<T>
    where
        T: Clone,
    {
        let (outcome, p) = self.most_likely();
        let confidence = p.into_confidence();
        if confidence.meets(threshold) {
            Decision::Commit {
                value: outcome.clone(),
                confidence,
            }
        } else {
            Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact invariant boundaries by design

    use super::*;
    use crate::intelligence::{AbstentionReason as AR, Probability as P};
    use alloc::vec;

    #[test]
    fn ranking_orders_by_confidence() {
        let ranking = Ranking::new(vec![
            Score::new("c", Confidence::new_or_panic(0.3)),
            Score::new("a", Confidence::new_or_panic(0.9)),
            Score::new("b", Confidence::new_or_panic(0.6)),
        ]);
        assert_eq!(ranking.best().unwrap().value(), &"a");
        assert_eq!(ranking.candidates().len(), 3);
        // Above threshold commits to the best; below abstains.
        assert_eq!(
            ranking.decide(Confidence::new_or_panic(0.8)),
            Decision::Commit {
                value: "a",
                confidence: Confidence::new_or_panic(0.9)
            }
        );
        assert_eq!(
            ranking.decide(Confidence::new_or_panic(0.95)),
            Decision::Abstain {
                reason: AR::InsufficientConfidence
            }
        );
        let empty: Ranking<u8> = Ranking::new(Vec::new());
        assert_eq!(
            empty.decide(Confidence::FULL),
            Decision::Abstain {
                reason: AR::NoCandidates
            }
        );
    }

    #[test]
    fn multi_label_decides_per_label() {
        let labels = MultiLabelDecision::new()
            .with_label("spam", true, Confidence::new_or_panic(0.9))
            .with_label("urgent", false, Confidence::new_or_panic(0.7))
            .with(
                "legal",
                Decision::Abstain {
                    reason: AR::InsufficientConfidence,
                },
            );
        assert_eq!(labels.value_of("spam"), Some(true));
        assert_eq!(labels.value_of("urgent"), Some(false));
        assert_eq!(labels.value_of("legal"), None);
        assert_eq!(labels.value_of("missing"), None);
        assert_eq!(labels.active_labels(), vec!["spam"]);
        // Replacing a label keeps one slot per label.
        let replaced = labels.with_label("spam", false, Confidence::NONE);
        assert_eq!(replaced.labels().len(), 3);
        assert_eq!(replaced.value_of("spam"), Some(false));
    }

    #[test]
    fn hypotheses_revise_bayesian_style() {
        let prior = Categorical::new([("fair", 1.0), ("loaded", 1.0)]).unwrap();
        let mut hypotheses = Hypotheses::from_prior(prior).unwrap();
        // Uniform prior: the leader's chance is 0.5 (the tie-break order is
        // unspecified, so assert the chance, not the label).
        assert_eq!(hypotheses.leader().1.value(), 0.5);

        // Evidence: three heads in a row is 8x more likely under "loaded".
        hypotheses = hypotheses
            .revise(|h| if *h == "loaded" { 8.0 } else { 1.0 })
            .unwrap();
        let (leader, p) = hypotheses.leader();
        assert_eq!(leader, &"loaded");
        assert_eq!(p.value(), 8.0 / 9.0);
        assert!((hypotheses.posterior().entropy_bits() - 0.503_258_334_775_645_4).abs() < 1e-9);
    }

    #[test]
    fn categorical_decide_uses_explicit_policy() {
        let dist = Categorical::new([("red", 3.0), ("blue", 1.0)]).unwrap();
        assert_eq!(
            dist.decide(Confidence::new_or_panic(0.7)),
            Decision::Commit {
                value: "red",
                confidence: Confidence::new_or_panic(0.75)
            }
        );
        assert_eq!(
            dist.decide(Confidence::new_or_panic(0.9)),
            Decision::Abstain {
                reason: AR::InsufficientConfidence
            }
        );
    }

    #[test]
    fn binary_and_records() {
        let yes: BinaryDecision = Decision::yes(Confidence::new_or_panic(0.9));
        assert_eq!(yes.committed(), Some(&true));
        let record = DecisionRecord::new(yes, Provenance::local());
        assert_eq!(record.decision().committed(), Some(&true));
        assert_eq!(record.provenance().origin, "local");
    }

    #[test]
    fn calibration_validates_ece() {
        let ok = Calibration {
            method: Some(String::from("temperature scaling")),
            reference: Some(String::from("holdout-2026-09")),
            expected_calibration_error: Some(0.03),
        };
        assert!(ok.validate().is_ok());
        let bad = Calibration {
            expected_calibration_error: Some(1.5),
            ..Calibration::default()
        };
        assert_eq!(bad.validate(), Err(crate::InvalidProbability(1.5)));
    }

    #[test]
    fn probability_type_remains_in_scope() {
        // Guards the import surface used by the docs above.
        let p: P = Probability::ONE;
        assert_eq!(p, Probability::ONE);
    }
}
