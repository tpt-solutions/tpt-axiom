//! halo2 backend benchmarks: circuit size, key generation, proving, and
//! verification for the shared conformance circuits. Scenario names match
//! `tpt-axiom-backend-arkworks`' bench IDs one-for-one so numbers can be
//! compared across backends.

// `criterion_group!` expansion trips nursery lints (generated harness
// helpers are non-const, undocumented functions).
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)] // criterion_group! generates undocumented harness fns

use criterion::{Criterion, criterion_group, criterion_main};
use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_zk::ZkBackend;
use tpt_axiom_zk::conformance::balance_transfer_ir;

fn bench_compile(c: &mut Criterion) {
    let backend = Halo2Backend;
    let ir = balance_transfer_ir();
    c.bench_function("balance_transfer/compile", |b| {
        b.iter(|| backend.compile(&ir).expect("compile"));
    });
}

fn bench_keygen(c: &mut Criterion) {
    let backend = Halo2Backend;
    let ir = balance_transfer_ir();
    c.bench_function("balance_transfer/keygen", |b| {
        b.iter(|| backend.generate_keys(&ir, &[]).expect("keys"));
    });
}

fn bench_prove(c: &mut Criterion) {
    let backend = Halo2Backend;
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
    let backend = Halo2Backend;
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

/// The circuit's row bound (2^k rows) and constraint counts, printed once via
/// `-- --verbose`-style runs: also asserted in `tests/proofs.rs`.
/// Size report only (same CLI surface as the timed benches).
fn bench_circuit_size(_c: &mut Criterion) {
    let ir = balance_transfer_ir();
    let k = tpt_axiom_backend_halo2::auto_k(&ir, 64);
    println!(
        "balance_transfer: {} exprs, {} constraints, 3 range-checked inputs, k = {k} ({} rows)",
        ir.exprs.len(),
        ir.constraints.len(),
        1usize << k,
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = bench_compile, bench_keygen, bench_prove, bench_verify, bench_circuit_size
}
criterion_main!(benches);
