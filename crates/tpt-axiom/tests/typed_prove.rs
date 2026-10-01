//! Typed end-to-end proving: the macro-generated `Inputs` struct plus the
//! `prove_named` / `verify_claim` driver, on a real halo2 proof.
//!
//! This is the whole Phase C usability path in one file: fill in a typed
//! struct, call `named()`, prove, verify — with the witness resolution,
//! constraint check, IR validation, and IR-digest binding all happening inside
//! the driver rather than in the caller's hands.

//!
//! This is the whole Phase C usability path in one file: fill in a typed
//! struct, call `named()`, prove, verify — with the witness resolution,
//! constraint check, IR validation, and IR-digest binding all happening inside
//! the driver rather than in the caller's hands.

#![cfg(feature = "halo2")]

use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{KeygenOptions, keygen, prove_named, verify_claim};

#[zk_provable(backend = "halo2")]
fn prove_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver = receiver_balance + amount;
    assert_eq!(new_receiver, receiver_balance + amount);
}

#[test]
fn typed_inputs_prove_and_verify_end_to_end() {
    let backend = tpt_axiom_backend_halo2::Halo2Backend;

    // Typed keygen: validated options instead of a raw byte blob.
    let options = KeygenOptions::defaults();
    let (pk, vk) = keygen(&backend, &ProveTransfer, options).expect("keygen");

    let inputs = ProveTransferInputs::new(50, 20, 30);
    let witness = inputs.named().expect("values fit the IR's scalar model");
    let claim = prove_named(&backend, &ProveTransfer, &pk, &witness).expect("prove");
    assert_eq!(claim.circuit(), "prove_transfer");
    assert_eq!(claim.publics(), &[50, 20]);

    assert!(
        verify_claim(&backend, &ProveTransfer, &vk, &claim).expect("verify"),
        "a genuine proof of a satisfied statement verifies"
    );
}

#[test]
fn a_typed_witness_that_violates_the_circuit_never_reaches_proving() {
    let backend = tpt_axiom_backend_halo2::Halo2Backend;
    let (pk, _vk) = keygen(&backend, &ProveTransfer, KeygenOptions::defaults()).expect("keygen");

    // `amount` exceeds `sender_balance`, which the circuit forbids.
    let inputs = ProveTransferInputs::new(10, 20, 30);
    let witness = inputs.named().expect("values fit the IR's scalar model");
    let error = prove_named(&backend, &ProveTransfer, &pk, &witness)
        .expect_err("an unsatisfiable statement must not produce a claim");
    assert!(
        error.to_string().contains("witness violates the circuit"),
        "unexpected diagnosis: {error}"
    );
}

#[test]
fn an_incomplete_witness_is_refused_by_name() {
    let backend = tpt_axiom_backend_halo2::Halo2Backend;
    let (pk, _vk) = keygen(&backend, &ProveTransfer, KeygenOptions::defaults()).expect("keygen");

    // A witness that names only some of the circuit's inputs.
    let partial = tpt_axiom_zk::NamedWitness::new().with("sender_balance", 50);
    let error = prove_named(&backend, &ProveTransfer, &pk, &partial)
        .expect_err("an incomplete witness must not produce a claim");
    assert!(
        error.to_string().contains("receiver_balance"),
        "the diagnosis must name the absent input: {error}"
    );
}
