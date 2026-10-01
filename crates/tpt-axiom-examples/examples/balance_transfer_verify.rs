//! End-to-end demo (verifier side): verify a proof produced by
//! `balance_transfer_prove` in a process that never runs any proving
//! code. The verifier rebuilds the circuit shape from the IR and regenerates
//! the verifying key deterministically (halo2 key generation consumes no
//! randomness; halo2 0.3 does not expose VK serialization), then checks the
//! shipped proof bytes against the shipped public inputs.
//!
//! ```text
//! cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_verify -- <out-dir>
//! ```

use std::path::{Path, PathBuf};

use halo2_proofs::pasta::Fp;
use halo2_proofs::plonk::{SingleVerifier, keygen_vk, verify_proof};
use halo2_proofs::poly::commitment::Params;
use halo2_proofs::transcript::{Blake2bRead, Challenge255};
use tpt_axiom_backend_halo2::{Halo2Circuit, auto_k, encode_scalar};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::CircuitDefinition;

/// The same circuit definition the prover used (in a real deployment both
/// sides compile it from the shared circuit crate).
#[zk_provable(backend = "halo2")]
/// # Panics
/// Panics if the transfer is invalid (overdraft), mirroring plain Rust.
pub fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

fn read_publics(path: &Path) -> Vec<i64> {
    std::fs::read_to_string(path)
        .expect("read publics.txt")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().parse::<i64>().expect("parse public input"))
        .collect()
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("balance-transfer-proof"), PathBuf::from);

    let proof_bytes = std::fs::read(dir.join("proof.bin")).expect("read proof.bin");
    let publics = read_publics(&dir.join("publics.txt"));

    // Rebuild the verifying key deterministically from the circuit IR.
    let ir = ProveBalanceTransfer.build();
    let k = auto_k(&ir, 64);
    let params = Params::<halo2_proofs::pasta::vesta::Affine>::new(k);
    let shape = Halo2Circuit::compile(&ir, 64, k);
    let vk = keygen_vk(&params, &shape).expect("verifying key generation");

    // Verify the shipped proof against the shipped public inputs. No proving
    // key, witness, or proving code is involved anywhere above this line.
    let instance: Vec<Fp> = publics.iter().map(|&v| encode_scalar(v)).collect();
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(proof_bytes.as_slice());
    let verdict = verify_proof(
        &params,
        &vk,
        SingleVerifier::new(&params),
        &[&[instance.as_slice()]],
        &mut transcript,
    );

    match verdict {
        Ok(()) => {
            println!(
                "VALID: prove_balance_transfer over publics {:?} (proof: {} bytes)",
                publics,
                proof_bytes.len()
            );
        }
        Err(e) => {
            eprintln!("INVALID: {e:?}");
            std::process::exit(1);
        }
    }
}
