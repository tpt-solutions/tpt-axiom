//! The link-time circuit registry: `#[zk_provable(..., register)]` circuits
//! are enumerated without any hand-maintained list, which is what lets a
//! `cargo axiom check`-style pass walk every circuit in a binary.

#![cfg(feature = "registry")]

use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, registry};

#[zk_provable(backend = "halo2", register)]
fn registered_transfer(#[public] sender_balance: u64, #[secret] amount: u64) {
    assert!(sender_balance >= amount);
}

#[zk_provable(backend = "arkworks", register)]
fn registered_bounds(#[secret] x: i64, #[public] hi: i64) {
    assert!(x <= hi);
}

// No `register` flag: this circuit must NOT appear in the registry.
#[zk_provable(backend = "sp1")]
fn unregistered_circuit(#[public] x: u64, #[public] hi: u64) {
    assert!(x <= hi);
}

fn names() -> Vec<&'static str> {
    registry::sorted().iter().map(|c| c.name).collect()
}

#[test]
fn registered_circuits_are_enumerable_without_a_manual_list() {
    assert_eq!(
        names(),
        ["registered_bounds", "registered_transfer"],
        "both `register` circuits are present, in sorted order"
    );
}

#[test]
fn a_circuit_without_the_register_flag_is_absent() {
    // The flag is opt-in: opting out must actually opt out, otherwise the
    // registry would over-report circuits the user did not ask to expose.
    assert!(
        !names().contains(&"unregistered_circuit"),
        "an unregistered circuit must not appear: {:?}",
        names()
    );
}

#[test]
fn a_registered_entry_rebuilds_its_own_ir() {
    // The entry holds a builder, not a cached system: what it reports is what
    // the circuit definition actually produces, so it cannot go stale.
    let entry = registry::find("registered_transfer").expect("registered");
    assert_eq!(entry.backend(), "halo2");
    let ir = entry.build();
    assert_eq!(ir.name, "registered_transfer");
    assert_eq!(ir.num_public(), 1);
    assert_eq!(ir.num_secret(), 1);
    // And it matches the definition directly.
    assert_eq!(ir, RegisteredTransfer.build());
    assert!(ir.validate().is_ok());
}

#[test]
fn lookup_of_an_unknown_name_lists_the_known_ones() {
    let error = registry::find("registerd_transfer").expect_err("misspelled");
    assert!(error.contains("registerd_transfer"), "{error}");
    assert!(error.contains("registered_transfer"), "{error}");
}

#[test]
fn describe_summarizes_every_registered_circuit() {
    let table = registry::describe();
    assert!(table.contains("registered_transfer"), "{table}");
    assert!(table.contains("registered_bounds"), "{table}");
    assert!(table.contains("[halo2]"), "{table}");
    assert!(table.contains("[arkworks]"), "{table}");
    assert!(table.contains("constraints"), "{table}");
}
