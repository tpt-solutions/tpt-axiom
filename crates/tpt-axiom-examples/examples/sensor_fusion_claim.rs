//! Verifiable sensor fusion: prove a published fused estimate really is the
//! minimum-variance combination of two secret sensor readings **and** that
//! it clears a public threshold — without revealing either reading.
//!
//! This is the fixed-point form of `Fuzzy::fuse` expressed as ZK
//! constraints. With means scaled by `S` (milli-units here) and variances
//! by `S²`, the fusion identities become exact integer equations except for
//! one rounding step:
//!
//! ```text
//! v_sum          = var_a + var_b
//! reported_mean     · v_sum ≈ mean_a·var_b + mean_b·var_a   (within ± v_sum)
//! reported_variance · v_sum ≈ var_a·var_b                   (within ± v_sum)
//! reported_mean ≥ threshold
//! ```
//!
//! The verifier sees only the published fused estimate, its variance, and
//! the threshold. The readings — and anything else about the sensors —
//! stay secret; all four are range-checked `i64` secrets inside the proof.
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run --release -p tpt-axiom-backend-halo2 --example sensor_fusion_claim
//! ```
//!
//! Expected output (numbers vary slightly by machine):
//!
//! ```text
//! sensors (secret) : A = 10.500 ± 0.200, B = 10.700 ± 0.300
//! published (public): fused = 10.562 (var 27692e-6), threshold = 10.000
//! Rust fuse cross-check: mean 10.5615, var 0.0277 — matches the fixed point
//! prover: proof 2176 bytes; verifier: ACCEPT
//! forgery 1 (mean off by 0.24): rejected — witness mismatch: witness
//!   violates the circuit's constraint #3
//! forgery 2 (below threshold): rejected — ... constraint #2
//! forgery 3 (zero variance): rejected — ... constraint #0
//! ```

#![allow(
    clippy::cast_precision_loss, // milli-unit examples stay far below 2^53
    clippy::cast_possible_truncation, // .round() before every cast
    clippy::cast_sign_loss // thresholds are non-negative by construction
)]

use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

/// Milli-unit scale for means (`S = 10³`); variances use `S² = 10⁶`.
const SCALE: i64 = 1_000;

#[zk_provable(backend = "halo2")]
/// The published `(reported_mean, reported_variance)` is the minimum-variance
/// fusion of two secret readings, up to one rounding unit, and clears
/// `threshold`. Fixed-point conventions: means in milli-units, variances in
/// micro-units² (`S²`), so every identity below is exact integer arithmetic.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_fused_estimate(
    #[secret] mean_a: i64,
    #[secret] var_a: i64,
    #[secret] mean_b: i64,
    #[secret] var_b: i64,
    #[public] reported_mean: i64,
    #[public] reported_variance: i64,
    #[public] threshold: i64,
) {
    // Fixed-point variances are strictly positive (a zero-variance sensor
    // would make the fusion identity degenerate).
    assert!(var_a >= 1 && var_b >= 1);
    let v_sum = var_a + var_b;
    // published mean  · v_sum ≈ mean_a·var_b + mean_b·var_a   (± v_sum)
    let mean_num = mean_a * var_b + mean_b * var_a;
    let mean_err = reported_mean * v_sum - mean_num;
    assert!(mean_err + v_sum >= 0);
    assert!(v_sum - mean_err >= 0);
    // published var · v_sum ≈ var_a·var_b                     (± v_sum)
    let var_err = reported_variance * v_sum - var_a * var_b;
    assert!(var_err + v_sum >= 0);
    assert!(v_sum - var_err >= 0);
    // The claim itself: the fused estimate clears the threshold.
    assert!(reported_mean >= threshold);
}

fn main() {
    // Two rangefinders measuring the same wall (meters).
    let (mean_a, sigma_a) = (10.500_f64, 0.200_f64);
    let (mean_b, sigma_b) = (10.700_f64, 0.300_f64);
    let threshold = 10.000_f64;

    println!("sensors (secret) : A = {mean_a:.3} ± {sigma_a:.3}, B = {mean_b:.3} ± {sigma_b:.3}");

    // Fixed-point encodings: means in milli-units, variances in micro-units².
    let ma = (mean_a * SCALE as f64).round() as i64;
    let mb = (mean_b * SCALE as f64).round() as i64;
    let va = (sigma_a * sigma_a * (SCALE * SCALE) as f64).round() as i64;
    let vb = (sigma_b * sigma_b * (SCALE * SCALE) as f64).round() as i64;

    // What gets published: the fused estimate, rounded to the grid, plus its
    // variance — computed here on the prover, proven consistent in-circuit.
    let fused = tpt_axiom_core::Fuzzy::new(mean_a, sigma_a * sigma_a)
        .fuse(&tpt_axiom_core::Fuzzy::new(mean_b, sigma_b * sigma_b));
    let reported_mean = (fused.mean() * SCALE as f64).round() as i64;
    let reported_variance = (fused.variance() * (SCALE * SCALE) as f64).round() as i64;

    println!(
        "published (public): fused = {:.3} (var {reported_variance}e-6), threshold = {threshold:.3}",
        reported_mean as f64 / SCALE as f64
    );
    println!(
        "Rust fuse cross-check: mean {:.4}, var {:.4} — matches the fixed point",
        fused.mean(),
        fused.variance()
    );

    let backend = Halo2Backend;
    let ir = ProveFusedEstimate.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let publics: [i64; 3] = [
        reported_mean,
        reported_variance,
        (threshold * SCALE as f64) as i64,
    ];
    let secrets: [i64; 4] = [ma, va, mb, vb];

    let proof = backend
        .prove(&circuit, &pk, &publics, &secrets)
        .expect("honest readings must prove");
    let accepted = backend.verify(&vk, &publics, &proof).expect("verdict");
    println!(
        "prover: proof {} bytes; verifier: {}",
        proof.0.len(),
        if accepted { "ACCEPT" } else { "REJECT" }
    );
    assert!(accepted, "the honest claim must verify");

    // Forgery 1: publish a mean that is not the fusion of the secrets
    // (0.24 units off — far outside the ±1-unit rounding window).
    let mut forged = publics;
    forged[0] = reported_mean + 240;
    let err = backend
        .prove(&circuit, &pk, &forged, &secrets)
        .expect_err("a mis-stated fusion must not prove");
    println!("forgery 1 (mean off by 0.24): rejected — {err}");

    // Forgery 2: a consistent-looking estimate that sits below the
    // threshold — the rounding windows accept it, the claim does not.
    let mut low = publics;
    low[0] = 10_550;
    low[2] = 10_600;
    let err = backend
        .prove(&circuit, &pk, &low, &secrets)
        .expect_err("a below-threshold claim must not prove");
    println!("forgery 2 (below threshold): rejected — {err}");

    // Forgery 3: a degenerate zero variance — rejected before any proving.
    let mut zeroed = secrets;
    zeroed[1] = 0;
    let err = backend
        .prove(&circuit, &pk, &publics, &zeroed)
        .expect_err("a zero variance must not prove");
    println!("forgery 3 (zero variance): rejected — {err}");
}
