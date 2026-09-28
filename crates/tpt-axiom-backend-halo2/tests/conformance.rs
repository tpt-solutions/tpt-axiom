//! Backend conformance: the shared scenario suite from `tpt-axiom-zk` run
//! against the halo2 adapter.

#[test]
fn conforms_to_zk_backend_contract() {
    tpt_axiom_zk::conformance::run_all(&tpt_axiom_backend_halo2::Halo2Backend);
}

#[test]
fn conformance_irs_match_macro_output() {
    use tpt_axiom_zk::CircuitDefinition;
    use tpt_axiom_zk::conformance::assert_variable_shape;

    assert_variable_shape(
        &ConformanceBalance.build(),
        &["sender_balance", "receiver_balance"],
        &["amount"],
    );
}

/// A macro-defined circuit mirroring the conformance IR, to prove the macro's
/// output classifies variables identically to the hand-built suite.
#[tpt_axiom_macros::zk_provable(backend = "halo2")]
fn conformance_balance(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}
