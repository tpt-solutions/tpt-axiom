//! Separate prover and verifier services: the deployment shape where the
//! secret data lives in one process, verification lives in another, and
//! nothing but a serialized [`tpt_axiom_zk::ProofEnvelope`] crosses the
//! wire.
//!
//! One binary plays all three roles over localhost TCP:
//!
//! * the **prover service** accepts a JSON witness (`{public: {...},
//!   secret: {...}}`), proves the balance-transfer circuit, and replies
//!   with the envelope as JSON;
//! * the **verifier service** accepts an envelope, rebuilds the circuit IR
//!   from its committed digest-binding name, and answers `true`/`false`;
//! * the **client** (this thread) calls the prover with the secret
//!   witness, forwards the returned envelope to the verifier, and prints
//!   the verdict. It then tampers with a public input inside the envelope
//!   and shows the verifier refusing it.
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run --release -p tpt-axiom-backend-halo2 --example proof_service
//! ```
//!
//! Expected output:
//!
//! ```text
//! prover service on 127.0.0.1:****, verifier service on 127.0.0.1:****
//! client: witness {sender_balance: 50, receiver_balance: 20, amount: 30}
//! client: envelope received (2112 bytes of proof) -> forwarded to verifier
//! verifier: ACCEPT
//! client: tampered envelope (receiver_balance 20 -> 21) -> forwarded
//! verifier: REJECT (proof does not match the claimed publics)
//! ```
//!
//! (The proof is ~2112 bytes; the envelope around it carries the backend
//! name, the circuit name, the IR digest, and the public inputs.)
//! ```

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ProofClaim, ProofEnvelope, ZkBackend};

#[zk_provable(backend = "halo2")]
/// The shared circuit, exactly as in the balance-transfer examples.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

/// The wire format the services speak: name-keyed JSON in, envelope JSON
/// out (or a plain error line).
fn read_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = String::new();
    stream.read_to_string(&mut buf)?;
    Ok(buf)
}

fn write_all(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    stream.write_all(bytes)
}

fn main() -> std::io::Result<()> {
    // Both services need the same circuit IR and keys (a real deployment
    // ships the verifying key; halo2 regenerates it deterministically from
    // the IR, see the backend's docs).
    let backend = Halo2Backend;
    let ir = ProveBalanceTransfer.build();
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // --- prover service: witness JSON in, envelope JSON out --------------
    let prover = TcpListener::bind("127.0.0.1:0")?;
    let verifier = TcpListener::bind("127.0.0.1:0")?;
    println!(
        "prover service on {}, verifier service on {}",
        prover.local_addr()?,
        verifier.local_addr()?
    );
    let prover_addr = prover.local_addr()?;
    let verifier_addr = verifier.local_addr()?;

    let prover_handle = thread::spawn(move || {
        let (mut stream, _) = prover.accept().expect("prover accept");
        let request = read_line(&mut stream).expect("prover read");
        // Name-keyed witness, resolved through the checked path: an
        // out-of-range, incomplete, or constraint-violating witness is
        // refused before any proving work.
        let witness: tpt_axiom_zk::named::NamedWitness =
            serde_json::from_str(&request).expect("witness JSON");
        let claim =
            tpt_axiom_zk::driver::prove_named(&backend, &ProveBalanceTransfer, &pk, &witness)
                .expect("honest witness must prove");
        let envelope = claim.to_envelope(&backend).expect("halo2 proofs are bytes");
        let body = serde_json::to_vec(&envelope).expect("serialize envelope");
        write_all(&mut stream, &body).expect("prover write");
        stream
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown");
    });

    // --- verifier service: envelope JSON in, verdict line out ------------
    let verifier_handle = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = verifier.accept().expect("verifier accept");
            let request = read_line(&mut stream).expect("verifier read");
            let verdict = (|| -> Option<bool> {
                let envelope: ProofEnvelope = serde_json::from_str(&request).ok()?;
                // Rebuild the circuit from the *claimed* name — in this
                // single-circuit service the only possibility is the
                // balance-transfer definition; a real verifier dispatches
                // on the name against its registry.
                if envelope.circuit != "prove_balance_transfer" {
                    return Some(false);
                }
                let ir = ProveBalanceTransfer.build();
                let rebuilt = ProofClaim::from_envelope(&backend, &envelope)?;
                rebuilt.verify_with(&backend, &vk, &ir).ok()
            })()
            .unwrap_or(false);
            let line = if verdict {
                "ACCEPT"
            } else {
                "REJECT (proof does not match the claimed publics)"
            };
            writeln!(stream, "{verdict}").expect("verifier write");
            stream
                .shutdown(std::net::Shutdown::Write)
                .expect("shutdown");
            println!("verifier: {line}");
        }
    });

    // --- client: prove remotely, verify remotely, then tamper ------------
    let mut prover_stream = TcpStream::connect(prover_addr)?;
    let witness = tpt_axiom_zk::named::NamedWitness::new()
        .with("sender_balance", 50_i64)
        .with("receiver_balance", 20_i64)
        .with("amount", 30_i64);
    println!("client: witness {{sender_balance: 50, receiver_balance: 20, amount: 30}}");
    write_all(
        &mut prover_stream,
        serde_json::to_string(&witness)
            .expect("serialize")
            .as_bytes(),
    )?;
    prover_stream.shutdown(std::net::Shutdown::Write)?;
    let mut response = String::new();
    prover_stream.read_to_string(&mut response)?;
    let envelope: ProofEnvelope = serde_json::from_str(&response).expect("envelope JSON");
    println!(
        "client: envelope received ({} bytes of proof) -> forwarded to verifier",
        envelope.proof.len()
    );

    let mut verifier_stream = TcpStream::connect(verifier_addr)?;
    write_all(&mut verifier_stream, response.as_bytes())?;
    verifier_stream.shutdown(std::net::Shutdown::Write)?;
    let mut verdict = String::new();
    verifier_stream.read_to_string(&mut verdict)?;
    assert!(
        verdict.starts_with("true"),
        "honest claim must be accepted: {verdict}"
    );

    // Tampering: claim a different receiver balance. The envelope's proof
    // is bound to the original publics, so the verifier refuses it.
    let mut tampered: ProofEnvelope = serde_json::from_str(&response).expect("envelope JSON");
    tampered.publics[1] = 21;
    println!("client: tampered envelope (receiver_balance 20 -> 21) -> forwarded");
    let mut verifier_stream = TcpStream::connect(verifier_addr)?;
    write_all(
        &mut verifier_stream,
        &serde_json::to_vec(&tampered).expect("serialize"),
    )?;
    verifier_stream.shutdown(std::net::Shutdown::Write)?;
    let mut verdict2 = String::new();
    verifier_stream.read_to_string(&mut verdict2)?;
    assert!(
        verdict2.starts_with("false"),
        "tampered envelope must be refused"
    );

    prover_handle.join().expect("prover thread");
    verifier_handle.join().expect("verifier thread");
    Ok(())
}
