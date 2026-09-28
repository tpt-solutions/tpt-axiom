//! End-to-end demo (prover side): prove `prove_balance_transfer` with real
//! halo2 IPA proofs and write the proof plus its public inputs to a directory
//! for the separate verifier binary (`balance_transfer_verify.rs`).
//!
//! ```text
//! cargo run -p tpt-axiom-backend-halo2 --example balance_transfer_prove -- <out-dir>
//! ```

use std::path::PathBuf;

use tpt_axiom_backend_halo2::{Halo2Backend, Halo2Circuit};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

/// The Phase 2 milestone circuit (spec.txt §4).
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

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("balance-transfer-proof"), PathBuf::from);
    std::fs::create_dir_all(&out_dir).expect("create output dir");

    // The transfer: 50 -> 20 + 30.
    let public = [50i64, 20];
    let secret = [30i64];

    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let circuit = Halo2Circuit::compile(&ir, 64, 0);
    let k = tpt_axiom_backend_halo2::auto_k(&ir, 64);
    let (pk, vk_shape) = backend.generate_keys(&ir, &[]).expect("key generation");
    let proof = backend
        .prove(&circuit, &pk, &public, &secret)
        .expect("proving");

    let proof_path = out_dir.join("proof.bin");
    std::fs::write(&proof_path, &proof.0).expect("write proof");
    let publics_path = out_dir.join("publics.txt");
    let mut publics = String::new();
    for v in public {
        publics.push_str(&v.to_string());
        publics.push('\n');
    }
    std::fs::write(&publics_path, publics).expect("write publics");
    let params_path = out_dir.join("params.txt");
    std::fs::write(
        &params_path,
        format!("backend = halo2\nk = {k}\nrange_bits = 64\n"),
    )
    .expect("write params");

    println!(
        "proved prove_balance_transfer(50, 20; amount = 30)\n  proof:    {}\n  publics:  {}\n  params:   {}\n  self-check: {}",
        proof_path.display(),
        publics_path.display(),
        params_path.display(),
        if backend
            .verify(&vk_shape, &public, &proof)
            .expect("self verify")
        {
            "verified"
        } else {
            "FAILED"
        },
    );
}
