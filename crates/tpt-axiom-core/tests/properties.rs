//! Property-based tests for the probabilistic invariants of the
//! intelligence types, plus the serialisation roundtrips that pin the
//! wire format (serde feature).

// Invariant boundaries are asserted exactly on validated values; generated
// weights intentionally cast small counters to `f64`.
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]

use proptest::prelude::*;
use tpt_axiom_core::{
    Categorical, Confidence, Decision, EmptyCategorical, Evidence, Probability, Provenance, Score,
    Validate,
};

prop_compose! {
    fn valid_probability()(v in 0.0_f64..=1.0) -> Probability {
        Probability::new_unchecked(v)
    }
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    #[test]
    fn complement_partitions_the_unit_interval(p in valid_probability()) {
        prop_assert!((p.value() + p.complement().value() - 1.0).abs() < 1e-12);
        // Double complementation is exact up to one f64 ULP.
        prop_assert!((p.complement().complement().value() - p.value()).abs() < 1e-15);
    }

    #[test]
    fn noisy_or_dominates_conjunct(
        p in valid_probability(),
        q in valid_probability(),
    ) {
        prop_assert!(p.noisy_or(q).value() >= p.value() - 1e-12);
        prop_assert!(p.noisy_or(q).value() >= q.value() - 1e-12);
        prop_assert!(p.conjunct(q).value() <= p.value() + 1e-12);
        prop_assert!(p.conjunct(q).value() <= q.value() + 1e-12);
    }

    #[test]
    fn categorical_normalizes_to_one(
        n in 1_usize..6,
        seed in 0.0_f64..42.0,
    ) {
        let weights: Vec<_> = (0..n)
            .map(|i| (i, (seed + i as f64).mul_add(0.5, 0.1).max(1e-6)))
            .collect();
        let dist = Categorical::new(weights).map_err(|_| TestCaseError::fail("build"))?;
        let total: f64 = dist.outcomes().iter().map(|(_, p)| p.value()).sum();
        prop_assert!((total - 1.0).abs() < 1e-9);
        prop_assert!(dist.validate().is_ok());
        // Entropy is bounded by log2(n).
        prop_assert!(dist.entropy_bits() <= (n as f64).log2() + 1e-9);
        prop_assert!(dist.entropy_bits() >= 0.0);
    }

    #[test]
    fn evidence_weights_stay_positive(
        w1 in 0.1_f64..10.0,
        w2 in 0.1_f64..10.0,
        w3 in 0.1_f64..10.0,
    ) {
        let e = Evidence::new("obs", w1, Provenance::local())
            .and_then(|e| e.combine_all([
                Evidence::new("x", w2, Provenance::local()).unwrap(),
                Evidence::new("y", w3, Provenance::local()).unwrap(),
            ]))
            .unwrap();
        prop_assert_eq!(e.weight(), w1 * w2 * w3);
        prop_assert!(e.weight() > 0.0 && e.weight().is_finite());
    }

    #[test]
    fn decision_policy_agrees_with_threshold(
        c in 0.0_f64..=1.0,
        t in 0.0_f64..=1.0,
    ) {
        let score = Score::new(1_u8, Confidence::new_unchecked(c));
        let threshold = Confidence::new_unchecked(t);
        let decision = Decision::from_score(score, threshold);
        prop_assert_eq!(decision.committed().is_some(), c >= t);
    }

    #[test]
    fn bayesian_update_renormalizes(
        w1 in 0.1_f64..4.0,
        w2 in 0.1_f64..4.0,
        l1 in 0.1_f64..4.0,
        l2 in 0.1_f64..4.0,
    ) {
        let prior = Categorical::new([("a", w1), ("b", w2)]).unwrap();
        let posterior = prior
            .bayesian_update(|o| if *o == "a" { l1 } else { l2 })
            .map_err(|_: EmptyCategorical| TestCaseError::fail("posterior"))?;
        let total: f64 = posterior.outcomes().iter().map(|(_, p)| p.value()).sum();
        prop_assert!((total - 1.0).abs() < 1e-9);
    }
}

#[cfg(feature = "serde")]
#[test]
fn serde_roundtrips_pin_the_v1_wire_format() {
    // Default serde representations are the stable v1 format; these
    // roundtrips break loudly if the wire shape ever changes.
    let p = Probability::new_unchecked(0.42);
    assert_eq!(serde_json::to_string(&p).unwrap(), "0.42");
    assert_eq!(serde_json::from_str::<Probability>("0.42").unwrap(), p);

    let score = Score::new("value", Confidence::new_unchecked(0.9));
    let wire = serde_json::to_string(&score).unwrap();
    assert_eq!(wire, r#"{"value":"value","confidence":0.9}"#);
    assert_eq!(serde_json::from_str::<Score<&str>>(&wire).unwrap(), score);

    let decision = Decision::<&str>::Commit {
        value: "x",
        confidence: Confidence::new_unchecked(0.5),
    };
    assert_eq!(
        serde_json::to_string(&decision).unwrap(),
        r#"{"Commit":{"value":"x","confidence":0.5}}"#
    );

    let dist = Categorical::new([("a", 1.0), ("b", 3.0)]).unwrap();
    let wire = serde_json::to_string(&dist).unwrap();
    let back: Categorical<&str> = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.probability_of(&"a").value(), 0.25);
}
