//! Link-time circuit registry, so a binary can enumerate the circuits linked
//! into it.
//!
//! `#[zk_provable]` generates a `register` function per circuit. Without a
//! registry, nothing can ask a program "which circuits do you contain?" — every
//! caller must know the type name, which is exactly the manual bookkeeping that
//! lets a stale circuit definition slip through. `cargo axiom check` needs the
//! opposite: walk every circuit in the binary and run the mismatch detector
//! over each.
//!
//! [`linkme`](https://docs.rs/linkme) collects those registrations into a
//! distributed slice at link time, so the set is whatever the linker included —
//! no build script, no generated list, and dead-code elimination works in the
//! user's favour.
//!
//! Each entry stores a *function pointer* that rebuilds the IR rather than a
//! cached `ConstraintSystem`: a cached copy could describe a different circuit
//! than the code actually builds, which is precisely the failure this registry
//! exists to catch.
//!
//! The module needs `std` and is gated behind the `registry` feature. Without
//! it, registration is a no-op and `registered()` is empty, so a build that
//! forgets the feature still compiles and runs.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_axiom_ir::ConstraintSystem;

/// One circuit registered at link time.
#[derive(Clone, Copy, Debug)]
pub struct RegisteredCircuit {
    /// The circuit's name, as the annotated function's identifier.
    pub name: &'static str,
    /// The backend selector from `#[zk_provable(backend = "...")]`.
    pub backend: &'static str,
    /// Re-derives the circuit's IR from its `CircuitDefinition`.
    pub build: fn() -> ConstraintSystem,
}

impl RegisteredCircuit {
    /// The circuit's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The selected backend.
    #[must_use]
    pub const fn backend(&self) -> &'static str {
        self.backend
    }

    /// Builds the circuit's IR.
    #[must_use]
    pub fn build(&self) -> ConstraintSystem {
        (self.build)()
    }
}

/// Every `#[zk_provable(..., register)]` circuit linked into this binary.
///
/// Contribute with `#[tpt_axiom_zk::linkme::distributed_slice]` from the
/// generated code; read through [`registered`] or [`sorted`].
#[linkme::distributed_slice]
pub static REGISTERED_CIRCUITS: [RegisteredCircuit] = [..];

/// Every circuit linked into this binary, in unspecified order.
///
/// Elements are contributed by `#[zk_provable(backend = "...", register)]`
/// across every crate linked into the binary, so this reflects the linker's
/// decisions: dead-code-eliminated circuits do not appear.
#[must_use]
pub fn registered() -> Vec<RegisteredCircuit> {
    REGISTERED_CIRCUITS.iter().copied().collect()
}

/// The registered circuits, sorted by name so output is reproducible.
///
/// Registration order is link order, which varies with build settings;
/// sorting keeps `cargo axiom check` output diffable across machines.
#[must_use]
pub fn sorted() -> Vec<RegisteredCircuit> {
    let mut circuits = registered();
    circuits.sort_by(|a, b| a.name.cmp(b.name));
    circuits
}
/// Looks a registered circuit up by name.
///
/// # Errors
///
/// Returns a message naming the circuits that *are* registered, because
/// "unknown circuit" is nearly always a typo and the available names are the
/// useful part of the answer.
pub fn find(name: &str) -> Result<RegisteredCircuit, String> {
    let circuits = sorted();
    circuits
        .iter()
        .find(|c| c.name == name)
        .copied()
        .ok_or_else(|| unknown_circuit_message(name, &circuits))
}

/// The diagnostic for an unregistered circuit name.
fn unknown_circuit_message(name: &str, registered: &[RegisteredCircuit]) -> String {
    if registered.is_empty() {
        return String::from(
            "no circuits are registered (build with the `registry` feature of tpt-axiom-zk \
             enabled)",
        );
    }
    let mut known: Vec<&str> = registered.iter().map(|c| c.name).collect();
    known.sort_unstable();
    let mut msg = String::from("unknown circuit `");
    msg.push_str(name);
    msg.push_str("`; registered circuits are: ");
    msg.push_str(&known.join(", "));
    msg
}

/// The registered circuits as a printable one-line-per-circuit table: name,
/// backend, and the shape of the IR — what `cargo axiom inspect` shows for a
/// whole binary.
#[must_use]
pub fn describe() -> String {
    use core::fmt::Write as _;

    let circuits = sorted();
    let mut out = String::new();
    if circuits.is_empty() {
        out.push_str("no circuits registered\n");
        return out;
    }
    for circuit in circuits {
        let ir = circuit.build();
        let _ = writeln!(
            out,
            "{}  [{}]  {} public / {} secret / {} constraints",
            circuit.name,
            circuit.backend,
            ir.num_public(),
            ir.num_secret(),
            ir.constraints.len()
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_reports_a_useful_message_when_empty() {
        // Without the `registry` feature nothing is registered, and the error
        // must say so rather than implying the circuit is misspelled.
        let error = find("prove_balance_transfer").expect_err("nothing is registered here");
        assert!(error.contains("registry"), "unexpected message: {error}");
    }

    #[test]
    fn the_unknown_circuit_message_lists_what_is_available() {
        let circuits = [
            RegisteredCircuit {
                name: "b_circuit",
                backend: "halo2",
                build: || unreachable!(),
            },
            RegisteredCircuit {
                name: "a_circuit",
                backend: "arkworks",
                build: || unreachable!(),
            },
        ];
        let message = unknown_circuit_message("typo", &circuits);
        assert!(message.contains("typo"), "{message}");
        // Sorted, so the message is stable across runs.
        assert!(
            message.contains("a_circuit, b_circuit"),
            "expected a sorted list: {message}"
        );
    }

    #[test]
    fn resolution_matches_on_name() {
        let circuits = [RegisteredCircuit {
            name: "some_circuit",
            backend: "halo2",
            build: || tpt_axiom_ir::ConstraintSystem::default(),
        }];
        assert_eq!(
            find_in(&circuits, "some_circuit").expect("registered").name,
            "some_circuit"
        );
        assert!(find_in(&circuits, "other").is_err());
    }

    /// The resolution half of [`find`], split out so it can be tested against
    /// an arbitrary set (the real registry is empty without the feature).
    fn find_in(circuits: &[RegisteredCircuit], name: &str) -> Result<RegisteredCircuit, String> {
        circuits
            .iter()
            .find(|c| c.name == name)
            .copied()
            .ok_or_else(|| unknown_circuit_message(name, circuits))
    }
}
