//! # tpt-axiom-cli
//!
//! `cargo axiom` — the build-time and operational driver for `tpt-axiom`.
//!
//! The command surface is deliberately split by *who* runs it. `doctor`,
//! `inspect`, and `check` need no proving stack: they report what is installed
//! and what the linked circuits look like. `keygen`, `prove`, and `verify` drive
//! a real backend, and are gated behind the `halo2` / `arkworks` features so a
//! verifier-only install does not pull in a prover's proving system.
//!
//! Every command that produces machine-readable output also accepts `--json`,
//! so CI can assert on the result instead of scraping text.
//!
//! ```no_run
//! # fn main() -> std::process::ExitCode { tpt_axiom_cli::run() }
//! ```
//!
//! ## Where the circuits come from
//!
//! `inspect` and `check` enumerate circuits from the link-time registry
//! (`tpt_axiom_zk::registry`), which only contains circuits compiled *into this
//! binary*. A user's own circuits live in their crate, not here — so the CLI is
//! useful directly for a binary that embeds it, and as a library for projects
//! that want the same commands over their own circuits.

use std::process::ExitCode;

mod commands;
mod inputs;

pub use commands::{Command, Outcome, ParamArgs, run_command};
pub use inputs::{InputError, InputFile, WitnessFile};

/// The `cargo axiom` version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs the CLI, returning the process exit code.
///
/// Exit codes follow the conventional shell contract: `0` success, `1` a clean
/// negative answer (a verification that returned `false`, a `check` that found
/// a mismatch), and `2` a usage or environment error. A script can therefore
/// distinguish "the proof is bad" from "the tool could not run".
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Invoked as a cargo subcommand, cargo passes `axiom` as the first
    // argument; strip it so `cargo axiom check` and `axiom check` agree.
    let args: &[String] = match args.first() {
        Some(first) if first == "axiom" => &args[1..],
        _ => &args,
    };
    match run_command(args) {
        Outcome::Success => ExitCode::SUCCESS,
        Outcome::Negative => ExitCode::from(1),
        Outcome::Usage(message) => {
            eprintln!("cargo-axiom: {message}");
            eprintln!("try `cargo axiom --help`");
            ExitCode::from(2)
        }
        Outcome::Failure(message) => {
            eprintln!("cargo-axiom: {message}");
            ExitCode::from(2)
        }
    }
}
