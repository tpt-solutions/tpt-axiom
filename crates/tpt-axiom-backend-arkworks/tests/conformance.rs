//! Backend conformance: the shared scenario suite from `tpt-axiom-zk` run
//! against the arkworks adapter.

#[test]
fn conforms_to_zk_backend_contract() {
    tpt_axiom_zk::conformance::run_all(&tpt_axiom_backend_arkworks::ArkworksBackend);
}

#[test]
fn conformance_irs_match_macro_output() {
    use tpt_axiom_zk::CircuitDefinition;
    use tpt_axiom_zk::conformance::assert_variable_shape;

    assert_variable_shape(&ConformanceSum.build(), &["a", "b", "return"], &["k"]);
}

/// A macro-defined circuit mirroring the conformance IR, to prove the macro's
/// output classifies variables identically to the hand-built suite.
#[tpt_axiom_macros::zk_provable(backend = "arkworks")]
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn conformance_sum(a: u64, b: u64, #[secret] k: u64) -> u64 {
    let scaled = a * k;
    scaled + b
}
