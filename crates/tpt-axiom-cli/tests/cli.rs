//! End-to-end `cargo axiom` tests: the real command surface, driven against a
//! real halo2 proof.
//!
//! Each circuit here is registered, so this test binary's registry is
//! non-empty — which is exactly the condition `inspect`, `check`, `prove`, and
//! `verify` need. The prove/verify round trip is a genuine cryptographic one,
//! not a mock.

use tpt_axiom_cli::{Outcome, run_command};
use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2", register)]
fn cli_transfer(#[public] sender: u64, #[public] receiver: u64, #[secret] amount: u64) {
    assert!(sender >= amount);
    let new_receiver = receiver + amount;
    assert_eq!(new_receiver, receiver + amount);
}

/// A second circuit, so the listing commands have something to enumerate.
#[zk_provable(backend = "halo2", register)]
fn cli_bounds(#[secret] x: i64, #[public] hi: i64) {
    assert!(x <= hi);
}

fn run(args: &[&str]) -> Outcome {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    run_command(&owned)
}

#[test]
fn check_validates_every_registered_circuit() {
    // No argument means "every circuit the linker kept" — the property the
    // registry exists to provide.
    assert_eq!(run(&["check"]), Outcome::Success);
    assert_eq!(run(&["check", "--json"]), Outcome::Success);
    assert_eq!(run(&["check", "cli_transfer"]), Outcome::Success);
}

#[test]
fn inspect_describes_a_registered_circuit() {
    assert_eq!(run(&["inspect", "cli_transfer"]), Outcome::Success);
    assert_eq!(
        run(&["inspect", "cli_transfer", "--json"]),
        Outcome::Success
    );
    // Listing every circuit.
    assert_eq!(run(&["inspect"]), Outcome::Success);
    assert_eq!(run(&["inspect", "--json"]), Outcome::Success);
}

#[test]
fn an_unknown_circuit_is_a_usage_error_listing_the_real_ones() {
    match run(&["inspect", "cli_transfr"]) {
        Outcome::Usage(message) => {
            assert!(message.contains("cli_transfr"), "{message}");
            assert!(message.contains("cli_transfer"), "{message}");
        }
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn doctor_reports_the_compiled_backends() {
    assert_eq!(run(&["doctor"]), Outcome::Success);
    assert_eq!(run(&["doctor", "--json"]), Outcome::Success);
}

#[test]
fn a_misspelled_flag_is_a_usage_error_not_a_panic() {
    assert!(matches!(run(&["inspect", "--jsn"]), Outcome::Usage(_)));
    assert!(matches!(run(&["no_such_command"]), Outcome::Usage(_)));
    // `--help` and `--version` are successes, not errors.
    assert_eq!(run(&["--help"]), Outcome::Success);
    assert_eq!(run(&["--version"]), Outcome::Success);
}

#[test]
fn an_out_of_range_range_bits_is_refused() {
    match run(&["keygen", "cli_transfer", "--range-bits", "0"]) {
        Outcome::Usage(message) => {
            assert!(message.contains("range_bits"), "{message}");
        }
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn keygen_reports_parameters_and_the_ir_digest() {
    assert_eq!(run(&["keygen", "cli_transfer"]), Outcome::Success);
    assert_eq!(run(&["keygen", "cli_transfer", "--json"]), Outcome::Success);
    // A narrower, provable range chain is accepted.
    assert_eq!(
        run(&["keygen", "cli_transfer", "--range-bits", "32"]),
        Outcome::Success
    );
}

/// A scratch directory unique to this test run.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt_axiom_cli_{name}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn prove_then_verify_round_trips_a_real_proof() {
    let dir = scratch("round_trip");
    let input = dir.join("input.json");
    let proof = dir.join("proof.json");
    std::fs::write(&input, br#"{"sender": 50, "receiver": 20, "amount": 30}"#)
        .expect("write input");

    assert_eq!(
        run(&[
            "prove",
            "cli_transfer",
            "--input",
            input.to_str().expect("path"),
            "--out",
            proof.to_str().expect("path"),
        ]),
        Outcome::Success,
        "proving a satisfiable statement must succeed"
    );
    assert!(proof.exists(), "the proof envelope must be written");

    assert_eq!(
        run(&[
            "verify",
            "--proof",
            proof.to_str().expect("path"),
            "cli_transfer",
        ]),
        Outcome::Success,
        "a genuine proof must verify"
    );
}

#[test]
fn a_proof_does_not_verify_against_a_different_circuit() {
    // The proof is about `cli_transfer`; asking `cli_bounds` to verify it must
    // be a clean no, not an error — that is what the IR-digest binding is for.
    let dir = scratch("wrong_circuit");
    let input = dir.join("input.json");
    let proof = dir.join("proof.json");
    std::fs::write(&input, br#"{"sender": 50, "receiver": 20, "amount": 30}"#)
        .expect("write input");
    assert_eq!(
        run(&[
            "prove",
            "cli_transfer",
            "--input",
            input.to_str().expect("path"),
            "--out",
            proof.to_str().expect("path"),
        ]),
        Outcome::Success
    );

    assert_eq!(
        run(&[
            "verify",
            "--proof",
            proof.to_str().expect("path"),
            "cli_bounds",
        ]),
        Outcome::Negative,
        "a proof must not verify as a different circuit's statement"
    );
}

#[test]
fn an_unsatisfiable_input_is_refused_before_proving() {
    let dir = scratch("unsatisfiable");
    let input = dir.join("input.json");
    let proof = dir.join("proof.json");
    // `amount` exceeds `sender`, which the circuit forbids.
    std::fs::write(&input, br#"{"sender": 10, "receiver": 20, "amount": 30}"#)
        .expect("write input");

    match run(&[
        "prove",
        "cli_transfer",
        "--input",
        input.to_str().expect("path"),
        "--out",
        proof.to_str().expect("path"),
    ]) {
        Outcome::Usage(message) => {
            assert!(message.contains("violates the circuit"), "{message}");
        }
        other => panic!("expected a usage error, got {other:?}"),
    }
    assert!(
        !proof.exists(),
        "no proof may be written for a false statement"
    );
}

#[test]
fn an_incomplete_input_names_the_missing_field() {
    let dir = scratch("incomplete");
    let input = dir.join("input.json");
    std::fs::write(&input, br#"{"sender": 50}"#).expect("write input");
    match run(&[
        "prove",
        "cli_transfer",
        "--input",
        input.to_str().expect("path"),
    ]) {
        Outcome::Usage(message) => {
            assert!(message.contains("receiver"), "{message}");
        }
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn a_missing_input_file_is_reported_with_its_path() {
    match run(&["prove", "cli_transfer", "--input", "no/such/file.json"]) {
        Outcome::Usage(message) => {
            assert!(message.contains("no/such/file.json"), "{message}");
        }
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn a_missing_proof_file_is_a_failure_not_a_verdict() {
    // Nothing was proved, so this is a tool error, not a "the proof is bad".
    assert!(matches!(
        run(&["verify", "--proof", "no/such/proof.json", "cli_transfer"]),
        Outcome::Failure(_)
    ));
}
