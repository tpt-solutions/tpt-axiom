//! An ML decision pipeline with abstention as a first-class outcome.
//!
//! The scenario: a classifier outputs per-outcome logits; the pipeline turns
//! them into a normalized distribution, applies an explicit confidence
//! policy, and — when no outcome clears the bar — **abstains** instead of
//! guessing. Every step is typed: logits become validated chances, chances
//! become confidences only through the documented reinterpretation, and the
//! abstention carries its reason.
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run -p tpt-axiom --example ml_decision_abstention
//! ```
//!
//! The logits here are produced inline; a pipeline reading them from a real
//! engine would cross `tpt-axiom-interop`'s `EngineOutput` boundary instead
//! (same types, plus validation and provenance on arrival).
//!
//! Expected output:
//!
//! ```text
//! strong model: billing 0.092, shipping 0.076, fraud 0.832
//! decision    : ESCALATE to review — fraud at 0.832 clears review, not commit
//! weak model  : billing 0.288, shipping 0.236, fraud 0.475
//! decision    : REJECT (abstain, reason: InsufficientConfidence) — best was fraud at 0.475
//!
//! (Policy: commit at ≥ 0.90, review in [0.60, 0.90).)
//! ```

use tpt_axiom::prelude::*;

/// The three-way policy the pipeline owner chose — thresholds are inputs to
/// the types, never baked into them.
const COMMIT_AT: f64 = 0.90;
const REVIEW_AT: f64 = 0.60;

fn decide(topic: &str, logits: &[f64; 3], outcomes: [&str; 3]) -> Escalation {
    // 1. Normalize raw logits into a validated, normalized distribution.
    let scores: Vec<(&str, f64)> = outcomes
        .iter()
        .copied()
        .zip(softmax(logits, 1.0))
        .collect();
    let dist = Categorical::new_strict(scores).expect("finite positive weights");

    print!("{topic:<12}: ");
    let parts: Vec<String> = dist
        .outcomes()
        .iter()
        .map(|(name, p)| format!("{name} {:.3}", p.value()))
        .collect();
    println!("{}", parts.join(", "));

    // 2. Apply the explicit policy: a chance read as confidence decides.
    let (best, chance) = dist.most_likely();
    let confidence = chance.into_confidence(); // the documented reinterpretation
    let commit_at = Confidence::new_or_panic(COMMIT_AT);
    let review_at = Confidence::new_or_panic(REVIEW_AT);

    match classify_by_confidence(confidence, commit_at, review_at) {
        Escalation::Accept => {
            // The commit decision itself, with the confidence it asserts.
            let decision: Decision<&str> = dist.decide(commit_at);
            println!(
                "decision    : COMMIT {best:?} at {:.3}",
                decision.committed().map(|_| confidence.value()).unwrap_or_default()
            );
            Escalation::Accept
        }
        Escalation::Review => {
            println!(
                "decision    : ESCALATE to review — {best} at {:.3} clears review, not commit",
                confidence.value()
            );
            Escalation::Review
        }
        Escalation::Reject => {
            // Abstention is a first-class outcome, with a reason.
            let decision: Decision<&str> = Decision::Abstain {
                reason: AbstentionReason::InsufficientConfidence,
            };
            let Decision::Abstain { reason } = decision else {
                unreachable!("we abstained");
            };
            println!(
                "decision    : REJECT (abstain, reason: {reason:?}) — best was {best} at {:.3}",
                confidence.value()
            );
            Escalation::Reject
        }
    }
}

fn main() {
    let outcomes = ["billing", "shipping", "fraud"];

    // A confident classifier: fraud dominates well past the review band.
    let verdict = decide("strong model", &[0.4, 0.2, 2.6], outcomes);
    assert_eq!(verdict, Escalation::Review);

    // A weak model: nothing clears even the review band → abstain.
    let verdict = decide("weak model", &[0.4, 0.2, 0.9], outcomes);
    assert_eq!(verdict, Escalation::Reject);
}
