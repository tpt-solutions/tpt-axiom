//! The boundary type for TPT inference runtimes (`tpt-gpu`,
//! `tpt-local-ai`, `tpt-spark`).
//!
//! Those runtimes are not published to crates.io yet, so this module defines
//! the *interface contract* rather than `From` impls against their types:
//! every runtime emits, per draw, a sampled value plus a log-space weight —
//! [`InferenceSample`] is exactly that shape. When the runtimes publish,
//! feature-gated `From<TheirType>` impls land here without touching the
//! rest of the ecosystem.

use tpt_axiom_core::{Confidence, Evidence, InvalidEvidenceWeight, Provenance, Score};

/// One sampled output from an inference runtime: a point value in
/// log-weight space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InferenceSample {
    /// The sampled (or argmax/mean) value the runtime produced.
    pub value: f64,
    /// The natural-log importance weight / log-likelihood of this sample.
    pub log_weight: f64,
}

impl InferenceSample {
    /// Wraps a value with its log weight.
    #[must_use]
    pub const fn new(value: f64, log_weight: f64) -> Self {
        Self { value, log_weight }
    }

    /// The linear-space importance weight `exp(log_weight)`.
    ///
    /// Weights from log space can legitimately overflow to
    /// [`f64::INFINITY`] for very strong evidence; downstream conversions
    /// reject that explicitly rather than silently clamping.
    #[must_use]
    pub fn weight(&self) -> f64 {
        self.log_weight.exp()
    }

    /// Converts the sample into weighted [`Evidence`] about the value.
    ///
    /// # Errors
    /// [`InvalidEvidenceWeight`] when the exponentiated weight is not finite
    /// and positive (log-weight overflow to infinity or `-inf` inputs).
    pub fn to_evidence(&self) -> Result<Evidence<f64>, InvalidEvidenceWeight> {
        Evidence::new(self.value, self.weight(), Provenance::local())
    }

    /// Converts the sample into a [`Score`] under an externally supplied
    /// confidence — inference runtimes produce values and weights; belief in
    /// a score is a policy input, never invented here.
    #[must_use]
    pub const fn to_score(&self, confidence: Confidence) -> Score<f64> {
        Score::new(self.value, confidence)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on validated inputs

    use super::*;

    #[test]
    fn samples_convert_to_weighted_evidence() {
        let sample = InferenceSample::new(42.0, (3.0_f64).ln());
        let evidence = sample.to_evidence().unwrap();
        assert_eq!(evidence.observation(), &42.0);
        assert!((evidence.weight() - 3.0).abs() < 1e-12);

        // Two equal samples fold to squared weight.
        let combined = sample
            .to_evidence()
            .unwrap()
            .combine(sample.to_evidence().unwrap())
            .unwrap();
        assert!((combined.weight() - 9.0).abs() < 1e-12);
    }

    #[test]
    fn overflowing_log_weights_are_rejected_not_clamped() {
        let sample = InferenceSample::new(1.0, 1.0e308);
        assert!(sample.to_evidence().is_err());
        let sample = InferenceSample::new(1.0, f64::NEG_INFINITY);
        assert!(sample.to_evidence().is_err());
    }

    #[test]
    fn scores_need_external_confidence() {
        let sample = InferenceSample::new(7.0, 0.0);
        let score = sample.to_score(Confidence::new_unchecked(0.8));
        assert_eq!(score.value(), &7.0);
        assert_eq!(score.confidence().value(), 0.8);
    }
}
