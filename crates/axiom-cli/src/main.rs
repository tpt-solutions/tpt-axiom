//! # axiom-cli
//!
//! The `tpt-axiom` build-time driver.
//!
//! Later phases use this binary to generate proving/verifying keys for
//! `#[zk_provable]` circuits, invoke verification (including the Phase 3
//! `tpt-telos` circuit-equivalence check), and drive the Phase 4 backends.
//!
//! **Status: Phase 0 scaffolding.** The command surface exists so scripts and
//! CI can call `axiom <command>` today; the commands themselves are stubs that
//! exit non-zero with a clear "not implemented yet" message.

use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some("--version") | Some("-V") | Some("version") => {
            println!("axiom-cli {VERSION}");
            ExitCode::SUCCESS
        }
        Some("keys") => not_implemented("keys", "Phase 4 (backend adapters)"),
        Some("verify") => not_implemented("verify", "Phase 3/4 (tpt-telos + backends)"),
        Some(other) => {
            eprintln!("axiom-cli: unknown command `{other}`");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn not_implemented(command: &str, phase: &str) -> ExitCode {
    eprintln!("axiom-cli: `{command}` is not implemented yet (arrives in {phase})");
    ExitCode::from(2)
}

fn print_help() {
    println!(
        "\
axiom-cli {VERSION} — tpt-axiom build-time driver

USAGE:
    axiom <COMMAND>

COMMANDS:
    keys        Generate proving/verifying keys for a circuit
    verify      Verify a proof (and, later, Rust <-> circuit equivalence)
    version     Print the version
    help        Print this help"
    );
}