//! Backend comparison: run the same `prove_balance_transfer` circuit through
//! both real adapters (halo2 IPA/Vesta and arkworks Groth16/BLS12-381) and
//! print compile / keygen / prove / verify wall-clock times plus circuit
//! sizes. For statistically sound numbers use the criterion benches
//! (`cargo bench -p tpt-axiom-backend-halo2 -p tpt-axiom-backend-arkworks`);
//! this example is the one-glance version.
//!
//! ```text
//! cargo run -p tpt-axiom-backend-halo2 --example backend_comparison
//! ```

// One-glance timing report; the criterion benches are the precise version.
#![allow(clippy::cast_precision_loss)]

use std::time::Instant;

use tpt_axiom_backend_arkworks::ArkworksBackend;
use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_zk::ZkBackend;
use tpt_axiom_zk::conformance::balance_transfer_ir;

fn measure<B: ZkBackend>(name: &str, backend: &B, ir: &tpt_axiom_ir::ConstraintSystem)
where
    B::Error: std::fmt::Debug,
{
    let t = Instant::now();
    let circuit = backend.compile(ir).expect("compile");
    let compile = t.elapsed();

    let t = Instant::now();
    let (pk, vk) = backend.generate_keys(ir, &[]).expect("keygen");
    let keygen = t.elapsed();

    let t = Instant::now();
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    let prove = t.elapsed();

    let t = Instant::now();
    let valid = backend.verify(&vk, &[50, 20], &proof).expect("verify");
    let verify = t.elapsed();

    assert!(valid, "{name}: benchmark proof must verify");
    println!(
        "{name:9} compile {compile:>8.1?} | keygen {keygen:>8.1?} | prove {prove:>8.1?} | verify {verify:>8.1?}"
    );
}

fn main() {
    let ir = balance_transfer_ir();
    println!(
        "prove_balance_transfer: {} exprs, {} IR constraints\n",
        ir.exprs.len(),
        ir.constraints.len()
    );
    measure("halo2", &Halo2Backend, &ir);
    measure("arkworks", &ArkworksBackend, &ir);
}
