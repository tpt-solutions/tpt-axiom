//! Generated from tpt-axiom-template: two starters, one probabilistic and
//! one zero-knowledge. Delete what you do not need.

use tpt_axiom::prelude::*;

// ---------------------------------------------------------------------------
// Starter 1: uncertainty-propagating arithmetic.
// ---------------------------------------------------------------------------

/// Fuses two distance estimates with `Fuzzy` arithmetic. Every `+`/`*`
/// propagates uncertainty automatically (assuming independent inputs).
fn fused_distance() -> Fuzzy<f64> {
    let reading_a = Fuzzy::new(10.5, 0.04); // 10.5 m ± 0.2
    let reading_b = Fuzzy::new(10.7, 0.09); // 10.7 m ± 0.3
    reading_a.fuse(&reading_b)
}

// ---------------------------------------------------------------------------
// Starter 2: a zero-knowledge circuit from a plain Rust function.
// ---------------------------------------------------------------------------

#[zk_provable(backend = "halo2")]
/// Prove the age claim without revealing the birth year.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_age_over(
    #[public] current_year: u32,
    #[public] min_age: u32,
    #[secret] birth_year: u32,
) {
    assert!(current_year - birth_year >= min_age);
}

fn main() {
    // Probabilistic side: ordinary arithmetic, uncertainty for free.
    let distance = fused_distance();
    println!("fused distance: {distance:.3}");

    // ZK side: prove, then verify, a real proof.
    let backend = Halo2Backend;
    let ir = ProveAgeOver.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[2026, 18], &[1991])
        .expect("honest witness must prove");
    let accepted = backend
        .verify(&vk, &[2026, 18], &proof)
        .expect("verifier yields a verdict");
    println!(
        "age proof: {} bytes, verifier: {}",
        proof.0.len(),
        if accepted { "ACCEPT" } else { "REJECT" }
    );
    assert!(accepted);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The starter circuit proves and verifies end to end — keep this green
    /// as you extend the project.
    #[test]
    fn age_proof_roundtrips() {
        let backend = Halo2Backend;
        let ir = ProveAgeOver.build();
        let circuit = backend.compile(&ir).expect("compile");
        let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
        let proof = backend.prove(&circuit, &pk, &[2026, 18], &[1991]).expect("prove");
        assert!(backend.verify(&vk, &[2026, 18], &proof).expect("verify"));
        // An underage witness is rejected before any proving work.
        assert!(backend.prove(&circuit, &pk, &[2026, 18], &[2012]).is_err());
    }

    #[test]
    fn fusion_tightens_uncertainty() {
        let fused = fused_distance();
        assert!(fused.variance() > 0.0);
        assert!(fused.variance() < 0.04, "fusing must tighten the estimate");
    }
}
