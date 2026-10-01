//! The `cargo axiom` command surface: parsing and execution.
//!
//! # Why the proving commands take parameters, not key files
//!
//! halo2 0.3 exposes no key serialization, and its key generation consumes no
//! randomness: a verifying key is *re-derived deterministically* from the
//! circuit IR plus the parameter set. So the honest CLI surface is to carry the
//! parameters (`--range-bits`, `--k`) rather than pretend to ship key files that
//! cannot be loaded. `keygen` reports the parameters it would use and the
//! circuit IR digest — the two things a prover and a verifier must agree on.
//!
//! `doctor`, `inspect`, and `check` need no proving stack and work in every
//! build; `keygen`, `prove`, and `verify` require the `halo2` feature and say so
//! explicitly when it is absent.

use std::fmt;
#[cfg(feature = "halo2")]
use std::path::Path;

use clap::{Parser, Subcommand};
#[cfg(feature = "halo2")]
use tpt_axiom_zk::ZkBackend;
use tpt_axiom_zk::registry;

use crate::VERSION;
#[cfg(feature = "halo2")]
use crate::inputs::WitnessFile;

/// `cargo axiom` — the tpt-axiom build-time and operational driver.
#[derive(Debug, Parser)]
#[command(
    name = "cargo-axiom",
    version,
    about = "tpt-axiom driver: inspect circuits, check them, prove and verify"
)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The key-generation parameters every proving command carries.
///
/// halo2 key generation is deterministic, so these parameters *are* the key
/// material's identity: a prover and a verifier must pass the same ones.
#[derive(Debug, clap::Args, Clone, Copy)]
pub struct ParamArgs {
    /// Width of the range-check chain, in bits (1..=64).
    #[arg(long, value_name = "BITS")]
    pub range_bits: Option<u32>,
    /// Log2 of the circuit's row bound; omit for automatic sizing.
    #[arg(long, value_name = "K")]
    pub k: Option<u32>,
}

impl ParamArgs {
    /// Validates them into typed options.
    ///
    /// # Errors
    ///
    /// [`Outcome::Usage`] when a width is zero or wider than 64, or when a row
    /// bound does not fit the backend parameter byte.
    pub fn options(&self) -> Result<tpt_axiom_zk::KeygenOptions, Outcome> {
        let base = self.range_bits.map_or_else(
            || Ok(tpt_axiom_zk::KeygenOptions::defaults()),
            tpt_axiom_zk::KeygenOptions::with_range_bits,
        );
        let base = base.map_err(|e| Outcome::Usage(e.to_string()))?;
        let Some(k) = self.k else {
            return Ok(base);
        };
        base.with_k(k).map_err(|e| Outcome::Usage(e.to_string()))
    }
}

/// The `cargo axiom` subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Report the version, compiled-in backends, and registered circuits.
    Doctor {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Describe a circuit's inputs and constraints; omit the name to list all.
    Inspect {
        /// The circuit to describe.
        name: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Validate every registered circuit, as a build-time gate would.
    Check {
        /// Restrict the check to one circuit.
        name: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Report the key-generation parameters and IR digest for a circuit.
    Keygen {
        /// The circuit.
        name: String,
        /// Key-generation parameters.
        #[command(flatten)]
        params: ParamArgs,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Prove a statement about a circuit's witness, writing a proof envelope.
    Prove {
        /// The circuit to prove.
        name: String,
        /// JSON file of named input values.
        #[arg(long, value_name = "PATH")]
        input: String,
        /// Where to write the proof envelope.
        #[arg(long, value_name = "PATH", default_value = "proof.json")]
        out: String,
        /// Key-generation parameters, which a verifier must match.
        #[command(flatten)]
        params: ParamArgs,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Verify a proof envelope against a circuit.
    Verify {
        /// The proof envelope to verify.
        #[arg(long, value_name = "PATH")]
        proof: String,
        /// The circuit the claim should be about.
        name: String,
        /// The parameters the prover used.
        #[command(flatten)]
        params: ParamArgs,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

/// How a command ended, and what the process should exit with.
///
/// `Negative` is deliberately distinct from `Failure`: a verification that
/// returned `false` is a legitimate answer about a proof, while a malformed
/// input file is a tool error. Scripts need to tell those apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command did what was asked.
    Success,
    /// The command ran and its answer was "no" (a proof did not verify).
    Negative,
    /// The command line or the inputs were wrong.
    Usage(String),
    /// The command could not complete (missing file, backend failure).
    Failure(String),
}

impl Outcome {
    /// Builds an [`Outcome::Failure`] from any displayable error.
    fn failed(error: impl fmt::Display) -> Self {
        Self::Failure(error.to_string())
    }
}

/// Parses `args` and runs the requested command.
///
/// # Panics
///
/// Never: argument errors become [`Outcome::Usage`], so a binary embedding the
/// CLI cannot be taken down by a bad command line.
#[must_use]
pub fn run_command(args: &[String]) -> Outcome {
    let parsed =
        Cli::try_parse_from(std::iter::once("cargo-axiom".to_owned()).chain(args.iter().cloned()));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(err) => {
            // clap's exit code distinguishes `--help`/`--version` (success,
            // already rendered) from a real usage error.
            if err.exit_code() == 0 {
                println!("{err}");
                return Outcome::Success;
            }
            return Outcome::Usage(err.render().to_string().trim_end().to_owned());
        }
    };
    match cli.command {
        Command::Doctor { json } => doctor(json),
        Command::Inspect { name, json } => inspect(name.as_deref(), json),
        Command::Check { name, json } => check(name.as_deref(), json),
        Command::Keygen { name, params, json } => keygen(&name, params, json),
        Command::Prove {
            name,
            input,
            out,
            params,
            json,
        } => prove(&name, &input, &out, params, json),
        Command::Verify {
            proof,
            name,
            params,
            json,
        } => verify(&proof, &name, params, json),
    }
}

/// The backend names compiled into this binary.
// A `cfg` on each element keeps the list and the compiled features in one
// place, with no conditional `mut`.
fn compiled_backends() -> Vec<&'static str> {
    const BACKENDS: &[&str] = &[
        #[cfg(feature = "halo2")]
        "halo2",
        #[cfg(feature = "arkworks")]
        "arkworks",
    ];
    BACKENDS.to_vec()
}

/// Looks a registered circuit up, mapping the registry's message to an outcome.
fn lookup(name: &str) -> Result<registry::RegisteredCircuit, Outcome> {
    registry::find(name).map_err(Outcome::Usage)
}

/// The hex-encoded IR digest, for display.
#[cfg(feature = "halo2")]
fn hex_digest(ir: &tpt_axiom_ir::ConstraintSystem) -> String {
    use core::fmt::Write as _;
    tpt_axiom_zk::ir_digest(ir)
        .iter()
        .fold(String::new(), |mut acc, byte| {
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// The message for a circuit whose backend the CLI cannot drive.
#[cfg(feature = "halo2")]
fn unsupported_backend(backend: &str) -> Outcome {
    Outcome::Failure(format!(
        "this CLI build drives `halo2`; the circuit selects `{backend}`, which is not wired up \
         here (the adapter crate ships; the CLI driver for it does not)"
    ))
}

/// The message for a proving command whose backend was not compiled in.
#[cfg(not(feature = "halo2"))]
fn backend_unavailable(command: &str) -> Outcome {
    Outcome::Failure(format!(
        "`{command}` needs a compiled-in backend. Rebuild with `--features halo2` (or \
         `--features full`). `cargo axiom doctor` lists what this build has."
    ))
}

/// `doctor`: what is installed, and what is linked in.
fn doctor(json: bool) -> Outcome {
    let circuits = registry::sorted();
    let backends = compiled_backends();
    if json {
        let value = serde_json::json!({
            "version": VERSION,
            "backends": backends,
            "circuits": circuits.iter().map(|c| c.name).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return Outcome::Success;
    }
    println!("tpt-axiom {VERSION}");
    println!("backends compiled in:");
    if backends.is_empty() {
        println!("  (none — rebuild with --features halo2 or --features full)");
    } else {
        for backend in &backends {
            println!("  {backend}");
        }
    }
    println!("registered circuits:");
    if circuits.is_empty() {
        println!(
            "  (none — this binary links no #[zk_provable(..., register)] circuits; the CLI is \
             most useful as a library over your own circuits)"
        );
    } else {
        for circuit in &circuits {
            let ir = circuit.build();
            println!(
                "  {}  [{}]  {} public / {} secret",
                circuit.name,
                circuit.backend(),
                ir.num_public(),
                ir.num_secret()
            );
        }
    }
    Outcome::Success
}

/// `inspect`: describe one circuit, or list them all.
fn inspect(name: Option<&str>, json: bool) -> Outcome {
    let Some(name) = name else {
        if json {
            let circuits: Vec<_> = registry::sorted()
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "backend": c.backend(),
                        "constraints": c.build().constraints.len(),
                    })
                })
                .collect();
            let value = serde_json::json!({ "circuits": circuits });
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
        } else {
            print!("{}", registry::describe());
        }
        return Outcome::Success;
    };

    let circuit = match lookup(name) {
        Ok(circuit) => circuit,
        Err(outcome) => return outcome,
    };
    let ir = circuit.build();
    if let Err(error) = ir.validate() {
        return Outcome::failed(format!("circuit `{name}` is malformed: {error}"));
    }
    let layout = tpt_axiom_zk::InputLayout::of(&ir);
    if json {
        let value = serde_json::json!({
            "name": ir.name,
            "backend": circuit.backend(),
            "constraints": ir.constraints.len(),
            "expressions": ir.exprs.len(),
            "public": layout.public_slots().iter().map(slot_json).collect::<Vec<_>>(),
            "secret": layout.secret_slots().iter().map(slot_json).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        println!("{layout}");
        println!(
            "  {} constraint(s), {} expression node(s)",
            ir.constraints.len(),
            ir.exprs.len()
        );
    }
    Outcome::Success
}

/// One input slot, as JSON.
fn slot_json(slot: &tpt_axiom_zk::InputSlot) -> serde_json::Value {
    serde_json::json!({
        "index": slot.index,
        "name": slot.name,
        "visibility": format!("{:?}", slot.visibility).to_lowercase(),
        "type": format!(
            "{}{}",
            if slot.int_type.signed { "i" } else { "u" },
            slot.int_type.bits
        ),
        "is_output": slot.is_output,
    })
}

/// `check`: validate every registered circuit, as a build-time gate would.
///
/// This is what the registry exists for: the set of circuits is whatever the
/// linker kept, so a newly added circuit is checked as soon as it is reachable
/// — there is no list to forget to update.
fn check(name: Option<&str>, json: bool) -> Outcome {
    let circuits = match name {
        Some(name) => match lookup(name) {
            Ok(circuit) => vec![circuit],
            Err(outcome) => return outcome,
        },
        None => registry::sorted(),
    };
    if circuits.is_empty() {
        let note = "no circuits are registered in this binary; nothing to check";
        if json {
            let value = serde_json::json!({
                "ok": true, "checked": [], "failures": [], "note": note,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
        } else {
            println!("cargo axiom check: {note}");
        }
        return Outcome::Success;
    }

    let mut failures = Vec::new();
    for circuit in &circuits {
        if let Err(error) = circuit.build().validate() {
            failures.push(format!("{}: {error}", circuit.name));
        }
    }
    let checked: Vec<&str> = circuits.iter().map(|c| c.name).collect();
    if json {
        let value = serde_json::json!({
            "ok": failures.is_empty(),
            "checked": checked,
            "failures": failures,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else if failures.is_empty() {
        println!("cargo axiom check: {} circuit(s) valid", checked.len());
        for name in &checked {
            println!("  ok  {name}");
        }
    } else {
        println!("cargo axiom check: {} failure(s)", failures.len());
        for failure in &failures {
            println!("  FAIL  {failure}");
        }
    }
    if failures.is_empty() {
        Outcome::Success
    } else {
        Outcome::Negative
    }
}

/// `keygen`: report the parameters and IR digest a prover and verifier must share.
///
/// halo2 0.3 does not serialize verifying keys — they are re-derived from the
/// circuit IR and the same parameters — so this command writes no key files. It
/// reports the two things that must agree instead, which is exactly the
/// information a key file would have carried. Key generation really runs, so an
/// unprovable circuit fails here rather than yielding parameters nobody can use.
#[cfg(feature = "halo2")]
fn keygen(name: &str, params: ParamArgs, json: bool) -> Outcome {
    let circuit = match lookup(name) {
        Ok(circuit) => circuit,
        Err(outcome) => return outcome,
    };
    if circuit.backend() != "halo2" {
        return unsupported_backend(circuit.backend());
    }
    let options = match params.options() {
        Ok(options) => options,
        Err(outcome) => return outcome,
    };
    let ir = circuit.build();
    let halo2 = tpt_axiom_backend_halo2::Halo2Backend;
    if let Err(error) = halo2.generate_keys_with_options(&ir, &options) {
        return Outcome::failed(format!("key generation failed: {error}"));
    }
    if json {
        let value = serde_json::json!({
            "ok": true,
            "circuit": name,
            "backend": "halo2",
            "range_bits": options.range_bits,
            "k": options.k,
            "digest": hex_digest(&ir),
            "keys_persisted": false,
            "note": "halo2 0.3 does not serialize keys; they are re-derived from the IR and \
                     these parameters, so no key file is written",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        println!("key parameters for {name} [halo2]");
        println!("  range_bits: {}", options.range_bits);
        println!(
            "  k:          {}",
            options
                .k
                .map_or_else(|| "automatic".to_owned(), |k| k.to_string())
        );
        println!("  ir digest:  {}", hex_digest(&ir));
        println!("  (halo2 0.3 does not serialize keys; they are re-derived from these inputs)");
    }
    Outcome::Success
}

/// `keygen` without the `halo2` feature compiled in.
#[cfg(not(feature = "halo2"))]
fn keygen(_name: &str, _params: ParamArgs, _json: bool) -> Outcome {
    backend_unavailable("keygen")
}

/// `prove`: produce a proof envelope for a named witness.
#[cfg(feature = "halo2")]
fn prove(name: &str, input: &str, out: &str, params: ParamArgs, json: bool) -> Outcome {
    let circuit = match lookup(name) {
        Ok(circuit) => circuit,
        Err(outcome) => return outcome,
    };
    if circuit.backend() != "halo2" {
        return unsupported_backend(circuit.backend());
    }
    let options = match params.options() {
        Ok(options) => options,
        Err(outcome) => return outcome,
    };
    let ir = circuit.build();

    let witness = match WitnessFile::read(Path::new(input)) {
        Ok(witness) => witness,
        Err(error) => return Outcome::Usage(error.to_string()),
    };
    let values = match witness.into_witness(&ir) {
        Ok(values) => values,
        Err(message) => return Outcome::Usage(message),
    };

    let halo2 = tpt_axiom_backend_halo2::Halo2Backend;
    let (pk, _vk) = match halo2.generate_keys_with_options(&ir, &options) {
        Ok(keys) => keys,
        Err(error) => return Outcome::failed(format!("key generation failed: {error}")),
    };
    let compiled = match halo2.compile(&ir) {
        Ok(compiled) => compiled,
        Err(error) => return Outcome::failed(format!("compilation failed: {error}")),
    };
    let proof = match halo2.prove(&compiled, &pk, values.public(), values.secret()) {
        Ok(proof) => proof,
        Err(error) => return Outcome::failed(format!("proving failed: {error}")),
    };
    let claim = tpt_axiom_zk::ProofClaim::new(name, &ir, values.public(), proof);
    let Some(envelope) = claim.to_envelope(&halo2) else {
        return Outcome::Failure("backend cannot encode proofs".to_owned());
    };
    let text = match serde_json::to_string_pretty(&envelope) {
        Ok(text) => text,
        Err(error) => return Outcome::failed(error),
    };
    if let Err(error) = std::fs::write(out, &text) {
        return Outcome::failed(format!("cannot write `{out}`: {error}"));
    }

    if json {
        let value = serde_json::json!({
            "ok": true,
            "circuit": name,
            "backend": "halo2",
            "public": values.public(),
            "proof": out,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        println!("proved {name} [halo2] -> {out}");
    }
    Outcome::Success
}

/// `prove` without the `halo2` feature compiled in.
#[cfg(not(feature = "halo2"))]
fn prove(_name: &str, _input: &str, _out: &str, _params: ParamArgs, _json: bool) -> Outcome {
    backend_unavailable("prove")
}

/// `verify`: check a proof envelope against the circuit it claims to be about.
///
/// The circuit is found in the registry and its IR rebuilt, so the claim's
/// committed IR digest is checked against the *live* definition — an envelope
/// for a weakened or different circuit verifies as a clean no, not an error.
#[cfg(feature = "halo2")]
fn verify(proof_path: &str, name: &str, params: ParamArgs, json: bool) -> Outcome {
    let circuit = match lookup(name) {
        Ok(circuit) => circuit,
        Err(outcome) => return outcome,
    };
    if circuit.backend() != "halo2" {
        return unsupported_backend(circuit.backend());
    }
    let options = match params.options() {
        Ok(options) => options,
        Err(outcome) => return outcome,
    };
    let text = match std::fs::read_to_string(Path::new(proof_path)) {
        Ok(text) => text,
        Err(error) => {
            return Outcome::Failure(format!("cannot read proof `{proof_path}`: {error}"));
        }
    };
    let envelope: tpt_axiom_zk::ProofEnvelope = match serde_json::from_str(&text) {
        Ok(envelope) => envelope,
        Err(error) => return Outcome::Usage(format!("invalid proof envelope: {error}")),
    };

    let ir = circuit.build();
    let halo2 = tpt_axiom_backend_halo2::Halo2Backend;
    let Some(claim) = tpt_axiom_zk::ProofClaim::from_envelope(&halo2, &envelope) else {
        return Outcome::Usage(format!(
            "not a readable `halo2` claim (envelope version {}, backend `{}`)",
            envelope.version, envelope.backend
        ));
    };
    let (_pk, vk) = match halo2.generate_keys_with_options(&ir, &options) {
        Ok(keys) => keys,
        Err(error) => return Outcome::failed(format!("key generation failed: {error}")),
    };
    let verified = match claim.verify_with(&halo2, &vk, &ir) {
        Ok(verified) => verified,
        Err(error) => return Outcome::failed(format!("verification error: {error}")),
    };

    if json {
        let value = serde_json::json!({
            "ok": verified,
            "circuit": name,
            "backend": "halo2",
            "public": envelope.publics,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else if verified {
        println!("verified: {name} [halo2] holds for {:?}", envelope.publics);
    } else {
        println!("not verified: the proof does not hold for this circuit");
    }
    // A proof that does not verify is `Negative`, not `Failure`: it is an
    // answer about the proof, not an error running the tool.
    if verified {
        Outcome::Success
    } else {
        Outcome::Negative
    }
}

/// `verify` without the `halo2` feature compiled in.
#[cfg(not(feature = "halo2"))]
fn verify(_proof: &str, _name: &str, _params: ParamArgs, _json: bool) -> Outcome {
    backend_unavailable("verify")
}
