//! The `cargo-axiom` binary: a thin wrapper over the library's [`tpt_axiom_cli::run`].
//!
//! Everything lives in the library so the command surface can be unit-tested
//! in-process; this file exists only to give cargo a subcommand to invoke.

use std::process::ExitCode;

fn main() -> ExitCode {
    tpt_axiom_cli::run()
}
