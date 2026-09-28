//! arkworks backend benchmarks: circuit size, key generation, proving, and
//! verification for the shared conformance circuits. Scenario names match
//! `tpt-axiom-backend-halo2`' bench IDs one-for-one so numbers can be
//! compared across backends.

// `criterion_group!` expansion trips nursery lints (generated harness
// helpers are non-const, undocumented functions).
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)] // criterion_group! generates undocumented harness fns

use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem, SynthesisMode};
use criterion::{Criterion, criterion_group, criterion_main};
use tpt_axiom_backend_arkworks::ArkworksBackend;
use tpt_axiom_zk::ZkBackend;
use tpt_axiom_zk::conformance::balance_transfer_ir;

fn bench_compile(c: &mut Criterion) {
    let backend = ArkworksBackend;
    let ir = balance_transfer_ir();
    c.bench_function("balance_transfer/compile", |b| {
        b.iter(|| backend.compile(&ir).expect("compile"));
    });
}

fn bench_keygen(c: &mut Criterion) {
    let backend = ArkworksBackend;
    let ir = balance_transfer_ir();
    c.bench_function("balance_transfer/keygen", |b| {
        b.iter(|| backend.generate_keys(&ir, &[]).expect("keys"));
    });
}

fn bench_prove(c: &mut Criterion) {
    let backend = ArkworksBackend;
    let ir = balance_transfer_ir();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, _vk) = backend.generate_keys(&ir, &[]).expect("keys");
    c.bench_function("balance_transfer/prove", |b| {
        b.iter(|| {
            backend
                .prove(&circuit, &pk, &[50, 20], &[30])
                .expect("prove")
        });
    });
}

fn bench_verify(c: &mut Criterion) {
    let backend = ArkworksBackend;
    let ir = balance_transfer_ir();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    c.bench_function("balance_transfer/verify", |b| {
        b.iter(|| {
            assert!(
                backend.verify(&vk, &[50, 20], &proof).expect("verify"),
                "benchmark proof must verify"
            );
        });
    });
}

/// The lowered R1CS constraint count, printed once for size comparison with
/// the halo2 row bound.
/// Size report only (same CLI surface as the timed benches).
fn bench_circuit_size(_c: &mut Criterion) {
    let ir = balance_transfer_ir();
    let circuit = tpt_axiom_backend_arkworks::ArkworksCircuit::compile(&ir, 64);
    let cs = ConstraintSystem::<ark_bls12_381::Fr>::new_ref();
    cs.set_mode(SynthesisMode::Setup);
    circuit
        .generate_constraints(cs.clone())
        .expect("synthesize");
    println!(
        "balance_transfer: {} exprs, {} IR constraints, {} R1CS constraints",
        ir.exprs.len(),
        ir.constraints.len(),
        cs.num_constraints(),
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = bench_compile, bench_keygen, bench_prove, bench_verify, bench_circuit_size
}
criterion_main!(benches);
