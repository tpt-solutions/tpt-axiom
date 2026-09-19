//! `axiom` — build-time driver for tpt-axiom: proving-key/verifying-key
//! generation and proof verification.
//!
//! Phase 0 scaffold: real key-generation and verification wiring lands
//! alongside the backend adapters in Phase 4.

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiom", about = "tpt-axiom build-time driver")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate proving and verifying keys for a compiled circuit.
    Keygen,
    /// Verify a proof against a verifying key.
    Verify,
}

// Phase 4 wires real, fallible key generation/verification behind these
// subcommands; keep the `Result` return type now so that lands without a
// signature change.
#[allow(clippy::unnecessary_wraps)]
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Keygen => {
            println!("axiom keygen: not yet implemented (see todo.md Phase 4)");
        }
        Command::Verify => {
            println!("axiom verify: not yet implemented (see todo.md Phase 4)");
        }
    }
    Ok(())
}
