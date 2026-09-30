//! Negative-payload tests: deserialization is an untrusted boundary, so a
//! wire value violating a type's invariant must be *rejected*, not
//! reconstructed as an invalid instance (serde feature).

#![cfg(feature = "serde")]

use tpt_axiom_core::{
    Bernoulli, Categorical, Confidence, Evidence, Fuzzy, Probability, Provenance, Score,
    Uncertain,
};

#[test]
fn probability_rejects_out_of_range_payloads() {
    for bad in ["-0.5", "1.5", "NaN"] {
        assert!(
            serde_json::from_str::<Probability>(bad).is_err(),
            "{bad} must not deserialize into a Probability"
        );
    }
}

#[test]
fn confidence_rejects_out_of_range_payloads() {
    for bad in ["-0.1", "2.0", "NaN"] {
        assert!(
            serde_json::from_str::<Confidence>(bad).is_err(),
            "{bad} must not deserialize into a Confidence"
        );
    }
}

#[test]
fn fuzzy_rejects_invalid_variance_payloads() {
    for bad in [r#"{"mean":1.0,"variance":-0.5}"#, r#"{"mean":1.0,"variance":"NaN"}"#] {
        assert!(
            serde_json::from_str::<Fuzzy<f64>>(bad).is_err(),
            "{bad} must not deserialize into a Fuzzy"
        );
    }
    // A valid payload round-trips and keeps its invariants.
    let back: Fuzzy<f64> = serde_json::from_str(r#"{"mean":3.0,"variance":0.25}"#).unwrap();
    assert_eq!(back, Fuzzy::new(3.0, 0.25));
}

#[test]
fn categorical_rejects_non_normal_payloads() {
    // Valid per-outcome chances, but they do not sum to one: the type
    // invariant (a normalized distribution) must be re-checked at the
    // boundary.
    let bad = r#"[{"a":0.2},{"b":0.3}]"#;
    assert!(
        serde_json::from_str::<Categorical<&str>>(bad).is_err(),
        "a non-normalized categorical must be rejected"
    );
}

#[test]
#[allow(clippy::float_cmp)] // round-tripped log weight is bit-exact
fn evidence_weights_survive_the_boundary() {
    // A zero log weight (weight 1.0) deserializes fine.
    let zero = r#"{"observation":"x","log_weight":0.0,"provenance":{"origin":"o","timestamp_secs":null,"revision":null}}"#;
    let ok: Evidence<&str> = serde_json::from_str(zero).unwrap();
    assert!((ok.weight() - 1.0).abs() < 1e-15);
    // A huge log weight is *accepted* (stored exactly) and reads back as an
    // infinite linear weight — the overflow lives in the data, not a panic.
    let huge = r#"{"observation":"x","log_weight":1.0e308,"provenance":{"origin":"o","timestamp_secs":null,"revision":null}}"#;
    let e: Evidence<&str> = serde_json::from_str(huge).unwrap();
    assert_eq!(e.log_weight(), 1.0e308);
    assert!(e.weight().is_infinite());
}

#[test]
fn bernoulli_and_nested_types_reject_bad_inner_payloads() {
    // Bernoulli carries a Probability: an invalid inner value fails the
    // whole deserialization.
    assert!(serde_json::from_str::<Bernoulli>("1.5").is_err());
    // Score carries a Confidence.
    let bad = r#"{"value":1,"confidence":3.0}"#;
    assert!(serde_json::from_str::<Score<u8>>(bad).is_err());
    // Uncertain's Estimated arm carries a Fuzzy.
    let bad = r#"{"Estimated":{"mean":1.0,"variance":-2.0}}"#;
    assert!(serde_json::from_str::<Uncertain<f64>>(bad).is_err());
}

#[test]
fn probability_normalizes_negative_zero() {
    let p: Probability = serde_json::from_str("-0.0").unwrap();
    assert_eq!(p, Probability::ZERO);
    let raw = serde_json::to_string(&p).unwrap();
    assert_eq!(raw, "0.0", "negative zero must not leak into the wire format");
    let _ = Provenance::local(); // keep the import meaningful
}
