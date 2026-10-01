//! Proof-carrying decisions: the record + claim bundle verifies end to end
//! against a real proof (halo2 feature).

#![cfg(feature = "halo2")]

use tpt_axiom::audit::ProofCarriedDecision;
use tpt_axiom::prelude::*;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::ir_digest;

#[zk_provable(backend = "halo2")]
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_threshold(#[public] value: u32, #[public] min: u32) {
    assert!(value >= min);
}

#[test]
fn carried_claim_verifies_and_refuses_foreign_circuits() {
    let backend = Halo2Backend;
    let ir = ProveThreshold.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend.prove(&circuit, &pk, &[42, 18], &[]).expect("prove");

    // The decision: commit to the threshold check passing, recorded with
    // provenance, and carrying the proof that justifies the statement.
    let decision: Decision<bool> = Decision::yes(Confidence::FULL);
    let record = DecisionRecord::new(
        decision,
        Provenance {
            origin: String::from("threshold-service"),
            revision: Some(String::from("1.0")),
            ..Provenance::default()
        },
    );
    let claim = ProofClaim::new("prove_threshold", &ir, &[42, 18], proof);
    let carried = ProofCarriedDecision::new(record, claim);

    // The audit trail: statement publics + a re-verifiable proof.
    assert_eq!(carried.claimed_publics(), &[42, 18]);
    assert_eq!(ir_digest(&ir), carried.claim().binding());
    assert!(
        carried.verify(&backend, &vk, &ir).expect("verify"),
        "the carried claim must verify against its own circuit"
    );

    // ...and refuse a different (weakened) definition.
    let mut weakened = ProveThreshold.build();
    weakened.constraints.clear();
    assert!(
        !carried
            .verify(&backend, &vk, &weakened)
            .expect("clean false"),
        "a proof must not verify against a different circuit"
    );
}
