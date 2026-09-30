//! The probabilistic type system for AI/decision workloads: first-class
//! types for chances, beliefs, outcomes, scores, decisions, and the
//! evidence that supports them.
//!
//! These types are deliberately narrower than [`Fuzzy`](crate::Fuzzy): where
//! `Fuzzy<T>` answers "what is the value and how uncertain is it?", the
//! intelligence types answer "what do we believe, how strongly, and on what
//! grounds?" Key semantic distinctions are enforced by the type system:
//!
//! * [`Probability`] is a *chance of an outcome* — validated to `[0, 1]`.
//! * [`Confidence`] is a *degree of belief in a result or model* — the same
//!   numeric range, but a different thing, and converting between the two
//!   requires an explicit, deliberate call.
//! * [`Decision`] makes abstention a first-class outcome instead of an
//!   ad-hoc `None`.
//!
//! All types are strongly typed, composable, backend-independent, and (with
//! the optional `serde` feature) serialisable.

// Style: this module prefers explicitness — named constructors
// (`Probability(0.0)` inside `impl Probability`), by-value evidence
// arguments that document ownership, and f64-backed types that predate
// inlined format args. The nursery lints below disagree with those choices;
// they are allowed once here rather than scattered per item.
#![allow(clippy::use_self)]
#![allow(clippy::derive_partial_eq_without_eq)] // f64 backing is not Eq
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::Fuzzy;
use num_traits::Float;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A validated probability: a chance of an outcome, in the closed interval
/// `[0, 1]`.
///
/// Construct with [`Probability::new`] (fallible) or
/// [`Probability::new_or_panic`] (panicking on invalid input). Arithmetic
/// that could leave the interval — such as naive addition of two
/// probabilities — is deliberately not exposed as implicit operators; combine
/// chances through [`Bernoulli`], [`Categorical`], or the explicit methods
/// below.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Probability(f64);

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Probability {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// An invalid probability was rejected at construction.
#[allow(clippy::derive_partial_eq_without_eq)] // f64 is not Eq
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidProbability(pub f64);

impl fmt::Display for InvalidProbability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "probability must lie in [0, 1], got {}", self.0)
    }
}

impl core::error::Error for InvalidProbability {}

impl Default for Probability {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Probability {
    /// The probability of an impossible event.
    pub const ZERO: Probability = Probability(0.0);
    /// The probability of a certain event.
    pub const ONE: Probability = Probability(1.0);

    /// Validates a chance in `[0, 1]`.
    ///
    /// A negative zero normalizes to `+0.0`, so two ways of writing "zero
    /// chance" compare and hash identically.
    ///
    /// # Errors
    /// [`InvalidProbability`] when `value` is negative, above one, or NaN.
    pub fn new(value: f64) -> Result<Self, InvalidProbability> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(if value == 0.0 { 0.0 } else { value }))
        } else {
            Err(InvalidProbability(value))
        }
    }

    /// Validates a chance in `[0, 1]`, panicking on invalid input.
    ///
    /// The name says what it does: validate, *or panic*. It is not an
    /// unchecked constructor — nothing here skips validation.
    ///
    /// # Panics
    /// Panics when `value` is outside `[0, 1]` or NaN.
    #[must_use]
    pub fn new_or_panic(value: f64) -> Self {
        Self::new(value).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The underlying value, guaranteed to be in `[0, 1]`.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// The chance of the complementary outcome: `1 - p`.
    #[must_use]
    pub const fn complement(self) -> Self {
        Self(1.0 - self.0)
    }

    /// The chance that *at least one* of two independent events occurs:
    /// `1 - (1-p)(1-q)` (noisy-OR).
    #[must_use]
    pub fn noisy_or(self, other: Self) -> Self {
        Self(1.0 - (1.0 - self.0) * (1.0 - other.0))
    }

    /// The chance that *both* independent events occur: `p * q`.
    #[must_use]
    pub fn conjunct(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }

    /// Reads this chance as a decision confidence (the explicit reverse of
    /// [`Confidence::into_probability`]).
    #[must_use]
    pub const fn into_confidence(self) -> Confidence {
        Confidence(self.0)
    }

    /// Linear pooling of two chances about the *same* event, weighted.
    ///
    /// # Errors
    /// [`InvalidProbability`] when the weights are negative or the weighted
    /// combination leaves `[0, 1]` (impossible with non-negative weights
    /// summing to one).
    pub fn pooled(self, other: Self, weight: f64) -> Result<Self, InvalidProbability> {
        if !(0.0..=1.0).contains(&weight) {
            return Err(InvalidProbability(weight));
        }
        Self::new(weight * self.0 + (1.0 - weight) * other.0)
    }
}

impl fmt::Display for Probability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<f64> for Probability {
    type Error = InvalidProbability;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl PartialOrd for Probability {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Probability {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Eq for Probability {}

/// A degree of belief in a *result, estimate, or model* — semantically
/// distinct from [`Probability`], which is the chance of an *outcome*.
///
/// A sensor fusion result can be `Confidence(0.99)` that the distance is
/// `10.0 m ± 0.1` while the *probability* of detecting a target is a separate
/// `Probability(0.8)`. Collapsing the two into one type erases exactly the
/// distinction downstream decision logic needs, so conversion is explicit:
/// [`Confidence::into_probability`] documents at the call site that a belief
/// is being *reinterpreted* as a chance.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Confidence(f64);

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// An invalid confidence was rejected at construction.
#[allow(clippy::derive_partial_eq_without_eq)] // f64 is not Eq
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidConfidence(pub f64);

impl fmt::Display for InvalidConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "confidence must lie in [0, 1], got {}", self.0)
    }
}

impl core::error::Error for InvalidConfidence {}

impl Confidence {
    /// No belief either way.
    pub const NONE: Confidence = Confidence(0.0);
    /// Complete belief.
    pub const FULL: Confidence = Confidence(1.0);

    /// Validates a confidence level in `[0, 1]`.
    ///
    /// # Errors
    /// [`InvalidConfidence`] when `value` is outside `[0, 1]` or NaN.
    pub fn new(value: f64) -> Result<Self, InvalidConfidence> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(InvalidConfidence(value))
        }
    }

    /// Validates a confidence level in `[0, 1]`, panicking on invalid input.
    ///
    /// The name says what it does: validate, *or panic*. It is not an
    /// unchecked constructor — nothing here skips validation.
    ///
    /// # Panics
    /// Panics when `value` is outside `[0, 1]` or NaN.
    #[must_use]
    pub fn new_or_panic(value: f64) -> Self {
        Self::new(value).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The underlying value, guaranteed to be in `[0, 1]`.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Reinterprets this belief as an outcome chance.
    ///
    /// The explicit name makes the semantic leap visible at the call site.
    #[must_use]
    pub const fn into_probability(self) -> Probability {
        Probability(self.0)
    }

    /// Propagates confidence through a conjunctive (AND) step: the weaker
    /// belief bounds the result, so this is `min`.
    #[must_use]
    pub const fn conjunct(self, other: Self) -> Self {
        Self(if self.0 <= other.0 { self.0 } else { other.0 })
    }

    /// Propagates confidence through a disjunctive (OR) step: `max`.
    #[must_use]
    pub const fn disjunct(self, other: Self) -> Self {
        Self(if self.0 >= other.0 { self.0 } else { other.0 })
    }

    /// True when the confidence meets `threshold`.
    #[must_use]
    pub const fn meets(self, threshold: Self) -> bool {
        self.0 >= threshold.0
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::NONE
    }
}

impl Eq for Confidence {}

impl Ord for Confidence {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Confidence {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A two-outcome distribution: an event with chance [`Probability`] of
/// occurring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Bernoulli {
    p: Probability,
}

impl Bernoulli {
    /// A two-outcome distribution with chance `p`.
    ///
    /// # Errors
    /// Propagates [`InvalidProbability`].
    pub fn new(p: f64) -> Result<Self, InvalidProbability> {
        Ok(Self {
            p: Probability::new(p)?,
        })
    }

    /// The chance of the event occurring.
    #[must_use]
    pub const fn p(&self) -> Probability {
        self.p
    }

    /// The chance of the event *not* occurring.
    #[must_use]
    pub const fn q(&self) -> Probability {
        self.p.complement()
    }

    /// The odds ratio `p / (1 - p)`; infinite at `p = 1`.
    #[must_use]
    pub fn odds(&self) -> f64 {
        self.p.value() / self.q().value()
    }

    /// The likelihood of observing `occurred`.
    #[must_use]
    pub const fn likelihood(&self, occurred: bool) -> Probability {
        if occurred { self.p } else { self.q() }
    }

    /// Draws one trial with the supplied uniform `(0, 1)` generator.
    #[must_use]
    pub fn sample(&self, mut uniform: impl FnMut() -> f64) -> bool {
        uniform() < self.p.value()
    }
}

/// A finite distribution over outcomes of type `T` with validated weights.
///
/// Construction normalizes the weights, so a `Categorical` is always a valid
/// distribution regardless of the scale the caller supplied.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Categorical<T> {
    outcomes: Vec<(T, Probability)>,
}

/// No finite outcomes were supplied to [`Categorical::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyCategorical;

impl fmt::Display for EmptyCategorical {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a categorical distribution needs at least one outcome")
    }
}

impl core::error::Error for EmptyCategorical {}

/// Why [`Categorical::new_strict`] rejected its input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CategoricalError {
    /// No outcomes were supplied.
    Empty,
    /// A weight was NaN, zero, or negative - the strict constructor does
    /// not silently drop them the way the tolerant [`Categorical::new`]
    /// does.
    InvalidWeight(f64),
    /// The accumulated total overflowed to infinity, so normalization is
    /// impossible in `f64`.
    Overflow,
}

impl fmt::Display for CategoricalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => {
                f.write_str("a categorical distribution needs at least one outcome")
            }
            Self::InvalidWeight(w) => {
                write!(f, "outcome weight must be finite and positive, got {w}")
            }
            Self::Overflow => f.write_str("outcome weights overflowed to infinity"),
        }
    }
}

impl core::error::Error for CategoricalError {}

impl From<EmptyCategorical> for CategoricalError {
    fn from(_: EmptyCategorical) -> Self {
        CategoricalError::Empty
    }
}

impl<T: PartialEq> Categorical<T> {
    /// Builds a distribution over `(outcome, weight)` pairs, normalizing the
    /// weights to sum to one.
    ///
    /// Non-positive and non-finite weights are dropped; if nothing remains,
    /// [`EmptyCategorical`] is returned. Duplicate outcomes keep their first
    /// position and accumulate weight.
    ///
    /// # Errors
    /// [`EmptyCategorical`] when no valid outcome/weight pair is supplied.
    pub fn new(outcomes: impl IntoIterator<Item = (T, f64)>) -> Result<Self, EmptyCategorical> {
        let mut collected: Vec<(T, f64)> = Vec::new();
        for (outcome, weight) in outcomes {
            if !(weight.is_finite() && weight > 0.0) {
                continue;
            }
            if let Some(slot) = collected.iter_mut().find(|(known, _)| *known == outcome) {
                slot.1 += weight;
            } else {
                collected.push((outcome, weight));
            }
        }
        let total: f64 = collected.iter().map(|(_, w)| w).sum();
        if collected.is_empty() || !total.is_finite() || total <= 0.0 {
            return Err(EmptyCategorical);
        }
        let outcomes = collected
            .into_iter()
            .map(|(outcome, w)| {
                // Invariant: w > 0 and finite, total > 0, so the ratio is in [0, 1].
                (outcome, Probability::new_or_panic(w / total))
            })
            .collect();
        Ok(Self { outcomes })
    }

    /// The strict counterpart of [`Self::new`]: every weight must be finite
    /// and positive (nothing is silently dropped), the accumulated total
    /// must not overflow, and at least one outcome must remain. Duplicate
    /// outcomes still merge into their first position.
    ///
    /// # Errors
    /// [`CategoricalError::InvalidWeight`] for the first NaN/zero/negative
    /// weight, [`CategoricalError::Overflow`] when the total overflows,
    /// [`CategoricalError::Empty`] on empty input.
    pub fn new_strict(
        outcomes: impl IntoIterator<Item = (T, f64)>,
    ) -> Result<Self, CategoricalError> {
        let mut collected: Vec<(T, f64)> = Vec::new();
        for (outcome, weight) in outcomes {
            if !weight.is_finite() || weight <= 0.0 {
                return Err(CategoricalError::InvalidWeight(weight));
            }
            if let Some(slot) = collected.iter_mut().find(|(known, _)| *known == outcome) {
                slot.1 += weight;
            } else {
                collected.push((outcome, weight));
            }
        }
        if collected.is_empty() {
            return Err(CategoricalError::Empty);
        }
        let total: f64 = collected.iter().map(|(_, w)| w).sum();
        if !total.is_finite() {
            return Err(CategoricalError::Overflow);
        }
        let outcomes = collected
            .into_iter()
            .map(|(outcome, w)| (outcome, Probability::new_or_panic(w / total)))
            .collect();
        Ok(Self { outcomes })
    }

    /// The outcomes with their (normalized) chances, in insertion order.
    #[must_use]
    pub fn outcomes(&self) -> &[(T, Probability)] {
        &self.outcomes
    }

    /// The chance assigned to `outcome`, or [`Probability::ZERO`].
    #[must_use]
    pub fn probability_of(&self, outcome: &T) -> Probability {
        self.outcomes
            .iter()
            .find(|(known, _)| known == outcome)
            .map_or(Probability::ZERO, |(_, p)| *p)
    }

    /// The highest-chance outcome and its chance.
    ///
    /// # Panics
    /// Never in practice: a `Categorical` always holds at least one outcome
    /// (enforced at construction).
    #[must_use]
    pub fn most_likely(&self) -> (&T, Probability) {
        let (outcome, p) = self
            .outcomes
            .iter()
            .max_by(|a, b| a.1.value().total_cmp(&b.1.value()))
            .expect("a categorical always holds at least one outcome");
        (outcome, *p)
    }

    /// The Shannon entropy in bits: `-Σ p·log2(p)`.
    #[must_use]
    pub fn entropy_bits(&self) -> f64 {
        self.outcomes
            .iter()
            .map(|(_, p)| {
                let pv = p.value();
                if pv <= 0.0 { 0.0 } else { -pv * pv.log2() }
            })
            .sum()
    }

    /// Bayesian update: reweights every outcome by `likelihood(outcome)` and
    /// renormalizes, producing the posterior distribution.
    ///
    /// # Errors
    /// [`EmptyCategorical`] when the revised weights are all non-positive or
    /// the likelihoods are not finite.
    pub fn bayesian_update(&self, likelihood: impl Fn(&T) -> f64) -> Result<Self, EmptyCategorical>
    where
        T: Clone,
    {
        let revised: Vec<(T, f64)> = self
            .outcomes
            .iter()
            .map(|(o, p)| ((*o).clone(), p.value() * likelihood(o)))
            .collect();
        Self::new(revised)
    }

    /// Draws one outcome with the supplied uniform `(0, 1)` generator.
    #[must_use]
    pub fn sample(&self, mut uniform: impl FnMut() -> f64) -> &T {
        let mut cumulative = 0.0;
        let draw = uniform();
        for (outcome, p) in &self.outcomes {
            cumulative += p.value();
            if draw < cumulative {
                return outcome;
            }
        }
        // Numerical guard: return the last outcome.
        &self.outcomes.last().expect("non-empty").0
    }
}

/// A scored output together with the confidence in the score — the typed
/// shape of "the model says `42`, strongly" rather than a bare number.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Score<T> {
    value: T,
    confidence: Confidence,
}

impl<T> Score<T> {
    /// Pairs a score value with the confidence in it.
    #[must_use]
    pub const fn new(value: T, confidence: Confidence) -> Self {
        Self { value, confidence }
    }

    /// The scored value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The confidence attached to the score.
    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Consumes the score into its parts.
    #[must_use]
    pub fn into_parts(self) -> (T, Confidence) {
        (self.value, self.confidence)
    }
}

/// Provenance metadata for a probabilistic result: where it came from and
/// under what conditions it was produced. Backend- and vendor-independent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Provenance {
    /// Free-form origin identifier (sensor id, model name, pipeline stage…).
    pub origin: alloc::string::String,
    /// Producer-reported UNIX timestamp, when available.
    pub timestamp_secs: Option<u64>,
    /// Producer revision (model version, calibration id…), when available.
    pub revision: Option<alloc::string::String>,
}

impl Provenance {
    /// Provenance for an unnamed local computation.
    #[must_use]
    pub fn local() -> Self {
        Self {
            origin: alloc::string::String::from("local"),
            timestamp_secs: None,
            revision: None,
        }
    }
}

/// An observation supporting (or undercutting) a probabilistic value,
/// carrying its own weight and provenance.
///
/// Weights are stored in **log space**: combining independent evidence adds
/// log-weights (naive-Bayes style, [`Evidence::combine`]) and cannot
/// overflow the way multiplying linear weights can - a combined weight
/// beyond `f64`'s range reads back as [`f64::INFINITY`] from
/// [`Evidence::weight`] rather than failing, and the log form stays exact
/// for downstream log-domain consumers.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Evidence<T> {
    observation: T,
    log_weight: f64,
    provenance: Provenance,
}

#[cfg(feature = "serde")]
impl<'de, T: serde::de::Deserialize<'de>> Deserialize<'de> for Evidence<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Shadow<T> {
            observation: T,
            log_weight: f64,
            provenance: Provenance,
        }
        let shadow = Shadow::<T>::deserialize(deserializer)?;
        Self::from_log_weight(shadow.observation, shadow.log_weight, shadow.provenance)
            .map_err(serde::de::Error::custom)
    }
}

impl<T> Evidence<T> {
    /// Wraps an observation with a positive likelihood-style weight.
    ///
    /// Weights above one support the hypothesis, below one undercut it.
    ///
    /// # Errors
    /// [`InvalidEvidenceWeight`] when the weight is not finite and positive.
    pub fn new(
        observation: T,
        weight: f64,
        provenance: Provenance,
    ) -> Result<Self, InvalidEvidenceWeight> {
        if weight.is_finite() && weight > 0.0 {
            Ok(Self {
                observation,
                log_weight: weight.ln(),
                provenance,
            })
        } else {
            Err(InvalidEvidenceWeight(weight))
        }
    }

    /// Wraps an observation with its natural-log weight directly - the form
    /// inference runtimes and probabilistic programs naturally produce.
    ///
    /// # Errors
    /// [`InvalidEvidenceWeight`] when `log_weight` is not finite (`-inf`
    /// would be a zero weight, `+inf` a weight no `f64` can scale to).
    pub fn from_log_weight(
        observation: T,
        log_weight: f64,
        provenance: Provenance,
    ) -> Result<Self, InvalidEvidenceWeight> {
        if log_weight.is_finite() {
            Ok(Self {
                observation,
                log_weight,
                provenance,
            })
        } else {
            // Report the *linear* weight in the error, matching `new`.
            Err(InvalidEvidenceWeight(log_weight.exp()))
        }
    }

    /// The observation.
    #[must_use]
    pub const fn observation(&self) -> &T {
        &self.observation
    }

    /// The likelihood-style weight, `exp(log_weight)`.
    ///
    /// A combined weight beyond `f64`'s range reads back as
    /// [`f64::INFINITY`]; the exact value stays available through
    /// [`Evidence::log_weight`].
    #[must_use]
    pub fn weight(&self) -> f64 {
        self.log_weight.exp()
    }

    /// The natural-log weight - the exact, overflow-free representation.
    #[must_use]
    pub const fn log_weight(&self) -> f64 {
        self.log_weight
    }

    /// Where this evidence came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Combines two independent pieces of evidence about the same hypothesis
    /// by adding log-weights (keeping the first observation).
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        Self {
            observation: self.observation,
            log_weight: self.log_weight + other.log_weight,
            provenance: self.provenance,
        }
    }

    /// Folds a sequence of independent evidence into `self`, adding
    /// log-weights left to right (keeping this observation and provenance).
    #[must_use]
    pub fn combine_all(self, others: impl IntoIterator<Item = Self>) -> Self {
        others.into_iter().fold(self, Evidence::combine)
    }
}

/// Threshold classification of a confidence level under an explicit
/// three-way policy.
///
/// Commit above `accept_at`, escalate (e.g. to review) in the band between
/// `accept_at` and `reject_at`, and reject below.
///
/// The policy parameters belong to the caller; this is only the primitive.
#[must_use]
pub fn classify_by_confidence(
    confidence: Confidence,
    accept_at: Confidence,
    reject_at: Confidence,
) -> Escalation {
    if confidence.meets(accept_at) {
        Escalation::Accept
    } else if confidence.meets(reject_at) {
        Escalation::Review
    } else {
        Escalation::Reject
    }
}

/// The outcome of [`classify_by_confidence`]: a threshold/escalation
/// primitive over confidence levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Escalation {
    /// Confidence is high enough to act automatically.
    Accept,
    /// Confidence falls in the escalation band; defer (review, second model).
    Review,
    /// Confidence is too low; do not act on this output.
    Reject,
}

/// Deterministic validation of probabilistic outputs: re-check a value's
/// invariants without any trust in the producer.
///
/// This is the verification-boundary primitive for the intelligence types —
/// everything checkable without cryptography (ranges, normalization) runs
/// here; proof-based verification stays behind `tpt-axiom-zk` and never
/// leaks into these types.
pub trait Validate {
    /// Validation failure: an invalid chance value or a broken structural
    /// invariant.
    type Error: core::fmt::Display;

    /// Re-checks the invariants.
    ///
    /// # Errors
    /// The first violated invariant, as the type's error.
    fn validate(&self) -> Result<(), Self::Error>;
}

impl Validate for Probability {
    type Error = InvalidProbability;

    fn validate(&self) -> Result<(), Self::Error> {
        Probability::new(self.0).map(|_: Probability| ())
    }
}

impl Validate for Confidence {
    type Error = InvalidConfidence;

    fn validate(&self) -> Result<(), Self::Error> {
        Confidence::new(self.0).map(|_: Confidence| ())
    }
}

impl<T: PartialEq> Validate for Categorical<T> {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.outcomes.is_empty() {
            return Err(ValidationError::Empty);
        }
        let mut total = 0.0;
        for (_, p) in &self.outcomes {
            if p.value() < 0.0 || p.value() > 1.0 {
                return Err(ValidationError::ChanceOutOfRange(p.value()));
            }
            total += p.value();
        }
        if (total - 1.0).abs() > 1e-9 {
            return Err(ValidationError::NotNormalized(total));
        }
        Ok(())
    }
}

/// Structural validation failures of a [`Categorical`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValidationError {
    /// No outcomes at all.
    Empty,
    /// An outcome's chance left `[0, 1]`.
    ChanceOutOfRange(f64),
    /// Chances did not sum to one (within `1e-9`).
    NotNormalized(f64),
}

impl core::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("categorical has no outcomes"),
            Self::ChanceOutOfRange(v) => write!(f, "outcome chance {v} outside [0, 1]"),
            Self::NotNormalized(v) => write!(f, "outcome chances sum to {v}, not 1"),
        }
    }
}

impl core::error::Error for ValidationError {}

/// Reproducibility metadata: what it would take to re-run the computation
/// that produced a probabilistic result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Reproducibility {
    /// Algorithm or pipeline identifier.
    pub algorithm: String,
    /// Producer version (crate/model/calibration revision).
    pub version: Option<String>,
    /// RNG seed for stochastic stages, when deterministic.
    pub seed: Option<u64>,
}

impl Reproducibility {
    /// Metadata for a fully deterministic, seedless computation.
    #[must_use]
    pub fn deterministic(algorithm: impl Into<String>) -> Self {
        Self {
            algorithm: algorithm.into(),
            version: None,
            seed: None,
        }
    }
}

/// An evidence weight was not finite and positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidEvidenceWeight(pub f64);

impl fmt::Display for InvalidEvidenceWeight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "evidence weight must be finite and positive, got {}",
            self.0
        )
    }
}

impl core::error::Error for InvalidEvidenceWeight {}

/// A typed probabilistic decision: commit to a value with a stated
/// confidence, or explicitly abstain.
///
/// Abstention is a first-class outcome — an AI pipeline that cannot decide
/// Below a confidence threshold says so through the type system instead of
/// smuggling an `Option::None` past the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Decision<T> {
    /// Commit to `value`, asserting `confidence` in the choice.
    Commit {
        /// The chosen value.
        value: T,
        /// How much the decider believes in the choice.
        confidence: Confidence,
    },
    /// Decline to decide; the reason travels with the decision.
    Abstain {
        /// Why the decision was declined (e.g. "insufficient confidence").
        reason: AbstentionReason,
    },
}

/// Why a [`Decision::Abstain`] happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AbstentionReason {
    /// The best candidate's confidence did not meet the required threshold.
    InsufficientConfidence,
    /// No candidate was available at all.
    NoCandidates,
}

impl<T> Decision<T> {
    /// Applies an explicit policy: commit to the score's value when its
    /// confidence meets `threshold`, otherwise abstain.
    ///
    /// This is the conversion from probabilistic output to deterministic
    /// decision the foundation requires — and the threshold is part of the
    /// caller's policy, never baked into the type.
    #[must_use]
    pub fn from_score(score: Score<T>, threshold: Confidence) -> Self {
        let confidence = score.confidence();
        if confidence.meets(threshold) {
            Self::Commit {
                value: score.into_parts().0,
                confidence,
            }
        } else {
            Self::Abstain {
                reason: AbstentionReason::InsufficientConfidence,
            }
        }
    }

    /// The committed value, if the decision is a commit.
    #[must_use]
    pub fn committed(&self) -> Option<&T> {
        match self {
            Self::Commit { value, .. } => Some(value),
            Self::Abstain { .. } => None,
        }
    }
}

/// A value whose uncertainty is part of the type: either exactly known
/// ([`Uncertain::Certain`]) or an estimate with propagated spread
/// ([`Uncertain::Estimated`]).
///
/// Uncertainty is preserved through [`Uncertain::map`] until the caller
/// explicitly discards it with [`Uncertain::point_estimate`] - the
/// foundation's "preserve uncertainty unless the caller asks otherwise"
/// rule, expressed as an API.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Uncertain<T: Float> {
    /// Exactly known; no spread to reason about.
    Certain(T),
    /// An estimate carrying a [`Fuzzy`] (mean, variance) uncertainty.
    Estimated(Fuzzy<T>),
}

impl<T: Float> Uncertain<T> {
    /// Wraps an exactly-known value.
    #[must_use]
    pub fn certain(value: T) -> Self {
        Self::Certain(value)
    }

    /// Wraps an estimate with the given mean and variance.
    ///
    /// # Panics
    /// Panics when `variance` is negative or NaN (via [`Fuzzy::new`]).
    #[must_use]
    pub fn estimated(mean: T, variance: T) -> Self {
        Self::Estimated(Fuzzy::new(mean, variance))
    }

    /// True when the value is exactly known.
    #[must_use]
    pub const fn is_certain(&self) -> bool {
        matches!(self, Self::Certain(_))
    }

    /// Maps the value (or estimate) through `f`, propagating uncertainty for
    /// the estimated case via `df` (the derivative at the operating point,
    /// for first-order error propagation).
    #[must_use]
    pub fn map(self, f: impl Fn(T) -> T, df: T) -> Self {
        match self {
            Self::Certain(v) => Self::Certain(f(v)),
            Self::Estimated(estimate) => {
                let scaled = estimate * df;
                Self::Estimated(Fuzzy::new(f(estimate.mean()), scaled.variance()))
            }
        }
    }

    /// Discards uncertainty: the mean for estimates, the value for certain
    /// values. The explicit name marks every loss of information.
    #[must_use]
    pub fn point_estimate(&self) -> T {
        match self {
            Self::Certain(v) => *v,
            Self::Estimated(estimate) => estimate.mean(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on validated inputs

    use super::*;

    #[test]
    fn uncertain_preserves_until_discarded() {
        let exact: Uncertain<f64> = Uncertain::certain(2.0);
        let estimate = Uncertain::estimated(10.0, 0.25);

        assert!(exact.is_certain());
        assert!(!estimate.is_certain());

        // Mapping keeps uncertainty attached (variance scales by df^2).
        let halved = estimate.map(|v| v / 2.0, 0.5);
        assert!(matches!(halved, Uncertain::Estimated(_)));
        assert_eq!(halved.point_estimate(), 5.0);

        // Only the explicit discard drops it.
        assert_eq!(estimate.point_estimate(), 10.0);
        assert_eq!(exact.point_estimate(), 2.0);
    }

    #[test]
    fn probability_validates_range() {
        assert_eq!(Probability::new(0.0), Ok(Probability::ZERO));
        assert_eq!(Probability::new(1.0), Ok(Probability::ONE));
        assert_eq!(Probability::new(-0.1), Err(InvalidProbability(-0.1)));
        assert_eq!(Probability::new(1.1), Err(InvalidProbability(1.1)));
        assert!(Probability::new(f64::NAN).is_err());
        assert_eq!(Probability::new_or_panic(0.3).complement().value(), 0.7);
    }

    #[test]
    fn probability_pools_linearly() {
        let a = Probability::new_or_panic(0.2);
        let b = Probability::new_or_panic(0.8);
        assert_eq!(a.pooled(b, 0.5).unwrap().value(), 0.5);
        assert!(a.pooled(b, -0.1).is_err());
    }

    #[test]
    fn confidence_is_distinct_from_probability() {
        // Same range, different type: no implicit conversion exists.
        let c = Confidence::new_or_panic(0.9);
        let p: Probability = c.into_probability();
        assert_eq!(p.value(), 0.9);
        assert!(Confidence::new(1.5).is_err());
        assert!(c.meets(Confidence::new_or_panic(0.8)));
        assert!(!c.meets(Confidence::new_or_panic(0.95)));
    }

    #[test]
    fn bernoulli_reasons_about_trials() {
        let coin = Bernoulli::new(0.6).unwrap();
        assert_eq!(coin.q().value(), 0.4);
        assert_eq!(coin.likelihood(true).value(), 0.6);
        assert_eq!(coin.likelihood(false).value(), 0.4);
        assert!((coin.odds() - 1.5).abs() < 1e-12);
        // Deterministic sampling at the extremes.
        assert!(coin.sample(|| 0.5));
        assert!(!coin.sample(|| 0.75));
        assert!(Bernoulli::new(1.5).is_err());
    }

    #[test]
    fn categorical_normalizes_and_ranks() {
        let dist = Categorical::new([("red", 2.0), ("green", 1.0), ("blue", 1.0)]).unwrap();
        assert_eq!(dist.probability_of(&"red").value(), 0.5);
        assert_eq!(dist.probability_of(&"purple"), Probability::ZERO);
        assert_eq!(
            dist.most_likely(),
            (&"red", Probability::new_or_panic(0.5))
        );
        // Entropy of (0.5, 0.25, 0.25) is 1.5 bits.
        assert!((dist.entropy_bits() - 1.5).abs() < 1e-9);
        // Duplicate outcomes accumulate; non-positive / NaN weights are dropped.
        let merged =
            Categorical::new([("x", 1.0), ("x", 1.0), ("y", 0.5), ("z", f64::NAN)]).unwrap();
        assert_eq!(merged.outcomes().len(), 2);
        assert_eq!(merged.probability_of(&"x").value(), 0.8);
        assert!(Categorical::<&str>::new(Vec::new()).is_err());
    }

    #[test]
    fn categorical_samples_partition_the_range() {
        let dist = Categorical::new([("a", 1.0), ("b", 3.0)]).unwrap();
        assert_eq!(dist.sample(|| 0.1), &"a");
        assert_eq!(dist.sample(|| 0.5), &"b");
        assert_eq!(dist.sample(|| 0.999), &"b");
    }

    #[test]
    fn decisions_apply_explicit_policies() {
        let strong = Score::new(42, Confidence::new_or_panic(0.9));
        let weak = Score::new(42, Confidence::new_or_panic(0.3));
        let threshold = Confidence::new_or_panic(0.8);

        let committed = Decision::from_score(strong, threshold);
        assert_eq!(committed.committed(), Some(&42));

        let abstained = Decision::from_score(weak, threshold);
        assert_eq!(
            abstained,
            Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence
            }
        );
        assert!(abstained.committed().is_none());
    }

    #[test]
    fn evidence_combines_multiplicatively() {
        let a = Evidence::new("sighting", 2.0, Provenance::local()).unwrap();
        let b = Evidence::new("track", 3.0, Provenance::local()).unwrap();
        let combined = a.combine(b);
        assert_eq!(combined.observation(), &"sighting");
        assert_eq!(combined.weight(), 6.0);
        assert_eq!(combined.log_weight(), 2.0f64.ln() + 3.0f64.ln());
        assert_eq!(combined.provenance().origin, "local");
        assert!(Evidence::new("x", 0.0, Provenance::local()).is_err());
        assert!(Evidence::new("x", -1.0, Provenance::local()).is_err());
        // Log weights from a runtime land exactly; overflow shows up in the
        // linear read-back, never as a failure.
        let huge = Evidence::from_log_weight("x", 1.0e308, Provenance::local()).unwrap();
        assert_eq!(huge.log_weight(), 1.0e308);
        assert!(huge.weight().is_infinite());
        // Weights whose *linear* product overflows combine exactly in log
        // space: e^700 · e^700 has no f64 representation, but its log is
        // 1400.
        let big = Evidence::from_log_weight("x", 700.0, Provenance::local()).unwrap();
        let fused = big.combine(Evidence::from_log_weight("x", 700.0, Provenance::local()).unwrap());
        assert_eq!(fused.log_weight(), 1400.0);
        assert!(fused.weight().is_infinite(), "linear read-back overflows");
        let _ = huge;
        assert!(Evidence::from_log_weight("x", f64::NEG_INFINITY, Provenance::local()).is_err());
    }

    #[test]
    fn categorical_strict_constructor_rejects_bad_weights() {
        use crate::CategoricalError;
        assert_eq!(
            Categorical::<&str>::new_strict([("a", 0.0), ("b", 1.0)]),
            Err(CategoricalError::InvalidWeight(0.0))
        );
        assert_eq!(
            Categorical::<&str>::new_strict([("a", -1.0)]),
            Err(CategoricalError::InvalidWeight(-1.0))
        );
        assert!(Categorical::<&str>::new_strict([("a", f64::NAN)]).is_err());
        assert_eq!(
            Categorical::<&str>::new_strict(Vec::new()),
            Err(CategoricalError::Empty)
        );
        // The tolerant constructor drops all of these silently instead.
        assert!(Categorical::new([("a", 0.0), ("b", 1.0)]).is_ok());
    }

    #[test]
    fn categorical_strict_constructor_rejects_overflow() {
        let huge = [f64::MAX, f64::MAX];
        assert_eq!(
            Categorical::<&str>::new_strict([("a", huge[0]), ("b", huge[1])]),
            Err(CategoricalError::Overflow)
        );
    }

    #[test]
    fn negative_zero_probability_normalizes() {
        let p = Probability::new(-0.0).unwrap();
        assert_eq!(p, Probability::ZERO);
        assert_eq!(p.value(), 0.0);
        // Sign bit is gone, not just numerically equal.
        assert!(!p.value().is_sign_negative());
    }

    #[test]
    fn confidence_orders_displays_and_defaults() {
        let low = Confidence::NONE;
        let mid = Confidence::new_or_panic(0.5);
        let high = Confidence::FULL;
        assert!(low < mid && mid < high);
        assert_eq!(low, Confidence::default());
        // Display renders the bare value (no implicit percent formatting).
        assert_eq!(alloc::format!("{high}"), "1");
        // Eq is sound because values are validated into [0, 1] (no NaN).
        let a = Confidence::new_or_panic(0.3);
        let b = Confidence::new_or_panic(0.3);
        assert_eq!(a, b);
    }
}
