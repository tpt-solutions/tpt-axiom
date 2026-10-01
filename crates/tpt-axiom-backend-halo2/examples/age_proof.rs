//! The README-lead example: prove you are old enough **without revealing
//! your age** (or your birth year, or anything else about you).
//!
//! The statement, as a Rust function:
//!
//! ```text
//! public: current_year, min_age      (what the verifier already knows)
//! secret: birth_year                 (what the prover keeps hidden)
//! claim:  current_year - birth_year >= min_age
//! ```
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run --release -p tpt-axiom-backend-halo2 --example age_proof
//! ```
//!
//! Expected output (timings vary by machine and profile):
//!
//! ```text
//! statement : current_year - birth_year >= min_age
//! public inputs : current_year=2026, min_age=18
//! secret witness: birth_year=1991 (never leaves the prover)
//! prover: circuit 256 rows (k=8), keygen 12.3ms, prove 8.9ms, proof 2048 bytes
//! verifier: ACCEPT (proof valid for the committed statement)
//! adversarial: a birth_year of 2012 (age 14) is rejected at prove time:
//!   witness mismatch: witness violates the circuit's constraint #0
//! ```

use std::time::Instant;

use tpt_axiom_backend_halo2::{Halo2Backend, Halo2Circuit, auto_k};
use tpt_axiom_ir::ConstraintSystem;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

#[zk_provable(backend = "halo2")]
/// The claim: `current_year - birth_year >= min_age`, with the birth year
/// sealed behind a range-checked secret and the surplus proven non-negative.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_age_over(#[public] current_year: u32, #[public] min_age: u32, #[secret] birth_year: u32) {
    assert!(current_year - birth_year >= min_age);
}

fn main() {
    let current_year: u32 = 2026;
    let min_age: u32 = 18;
    let birth_year: u32 = 1991; // age 35 — comfortably over, and still private

    println!("statement : current_year - birth_year >= min_age");
    println!("public inputs : current_year={current_year}, min_age={min_age}");
    println!("secret witness: birth_year={birth_year} (never leaves the prover)");

    let backend = Halo2Backend;
    let ir: ConstraintSystem = ProveAgeOver.build();

    // --- prover side -----------------------------------------------------
    let start = Instant::now();
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let keygen = start.elapsed();
    let circuit: Halo2Circuit = backend.compile(&ir).expect("compile");
    let start = Instant::now();
    let proof = backend
        .prove(
            &circuit,
            &pk,
            &[i64::from(current_year), i64::from(min_age)],
            &[i64::from(birth_year)],
        )
        .expect("honest witness must prove");
    let prove = start.elapsed();
    let k = auto_k(&ir, 64);
    println!(
        "prover: circuit {} rows (k={k}), keygen {:.1?}, prove {:.1?}, proof {} bytes",
        1usize << k,
        keygen,
        prove,
        proof.0.len()
    );

    // --- verifier side (separate process in the balance-transfer example;
    // the same calls run here for the walkthrough) ------------------------
    let accepted = backend
        .verify(&vk, &[i64::from(current_year), i64::from(min_age)], &proof)
        .expect("verify yields a verdict");
    println!(
        "verifier: {}",
        if accepted {
            "ACCEPT (proof valid for the committed statement)"
        } else {
            "REJECT"
        }
    );

    // --- what a cheating prover looks like -------------------------------
    // An underage birth year violates `current_year - birth_year >= min_age`;
    // the IR re-check rejects it *before* any proving work, naming the
    // constraint. (Bypassing that check cannot help: the PLONKish
    // NonNegative range check fails synthesis.)
    let underage = backend.prove(
        &circuit,
        &pk,
        &[i64::from(current_year), i64::from(min_age)],
        &[i64::from(2012)],
    );
    match underage {
        Err(e) => {
            println!(
                "adversarial: a birth_year of 2012 (age 14) is rejected at prove time:\n  {e}"
            );
        }
        Ok(_) => panic!("an underage witness must not prove"),
    }
}
