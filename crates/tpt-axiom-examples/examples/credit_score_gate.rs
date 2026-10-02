//! A private credit-score gate: prove an applicant's score clears the
//! lending threshold **without revealing the score** (or the scorecard's
//! internal weights).
//!
//! The scorecard is fixed and public (that is what the regulator audits):
//! `score = 300 + base·income_band + history·on_time_payments`, where
//! `income_band` is the applicant's income in hundreds (banded before
//! proving, so the raw score provably lands in `[300, 850]` — a circuit is
//! straight-line arithmetic and has no `min`/`max`; the range is *proven*,
//! not clamped). What stays secret is the applicant's data. The circuit
//! proves `score >= threshold` over the *declared* integer types, so a
//! lender learns only "approved" — the score itself, income, and payment
//! history never leave the prover.
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run --release -p tpt-axiom-backend-halo2 --example credit_score_gate
//! ```
//!
//! Expected output (numbers vary slightly by machine):
//!
//! ```text
//! applicant (secret): income_band=52 (5200/mo), on_time=41
//! public: threshold=660, scorecard base=2 history=8 max=850
//! prover: score = 732 (never published); proof 2176 bytes
//! verifier: ACCEPT — score >= 660, score undisclosed
//! near-miss applicant: score 648 rejected at prove time (witness violates
//!   the circuit's constraint #2)
//! ```

use tpt_axiom_backend_halo2::Halo2Backend;
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

#[zk_provable(backend = "halo2")]
/// The public scorecard applied to secret inputs. `income_band` is income
/// in hundreds, which keeps the raw score inside `[300, score_cap]` for
/// every in-band input — the circuit *proves* that range rather than
/// clamping to it.
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_score_over(
    #[secret] income_band: i64,
    #[secret] on_time_payments: i64,
    #[public] base_weight: i64,
    #[public] history_weight: i64,
    #[public] score_cap: i64,
    #[public] threshold: i64,
) {
    // The scorecard.
    let score = 300 + base_weight * income_band + history_weight * on_time_payments;
    // Range membership of the score itself, then the gate.
    assert!(score >= 300 && score <= score_cap);
    assert!(score >= threshold);
}

fn main() {
    // Secret applicant data — the prover's private inputs. Income is
    // banded to hundreds before proving (part of the scorecard spec).
    let income = 5_200_i64;
    let income_band = income / 100;
    let on_time = 41_i64;
    // Public scorecard (fixed by policy, auditable by anyone).
    let (base_weight, history_weight, score_cap, threshold) = (2_i64, 8_i64, 850_i64, 660_i64);

    // The plain-Rust computation (kept by the macro) — what the prover runs
    // to *decide* whether proving is worth it.
    let score = 300 + base_weight * income_band + history_weight * on_time;
    debug_assert!(score >= 300 && score <= score_cap);
    println!("applicant (secret): income_band={income_band} ({income}/mo), on_time={on_time}");
    println!(
        "public: threshold={threshold}, scorecard base={base_weight} history={history_weight} max={score_cap}"
    );

    let backend = Halo2Backend;
    let ir = ProveScoreOver.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    let publics: [i64; 4] = [base_weight, history_weight, score_cap, threshold];
    let secrets: [i64; 2] = [income_band, on_time];

    let proof = backend
        .prove(&circuit, &pk, &publics, &secrets)
        .expect("an honest applicant must prove");
    let accepted = backend.verify(&vk, &publics, &proof).expect("verdict");
    println!(
        "prover: score = {score} (never published); proof {} bytes",
        proof.0.len()
    );
    println!(
        "verifier: {}",
        if accepted {
            "ACCEPT — score >= 660, score undisclosed"
        } else {
            "REJECT"
        }
    );
    assert!(accepted);

    // A near-miss applicant (income 3800 → band 38, 34 on-time →
    // score 648): the constraint fails *before* any proving work, at the
    // gate itself.
    let near_miss: [i64; 2] = [38, 34];
    let near_score = 300 + base_weight * 38 + history_weight * 34;
    let err = backend
        .prove(&circuit, &pk, &publics, &near_miss)
        .expect_err("a below-threshold applicant must not prove");
    println!("near-miss applicant: score {near_score} rejected at prove time ({err})");

    // A tampered scorecard (threshold quietly raised past the score) is
    // also just a failed constraint — and the verifier sees the public
    // threshold, so the move is visible anyway.
    let mut tampered = publics;
    tampered[3] = 750;
    assert!(
        backend.prove(&circuit, &pk, &tampered, &secrets).is_err(),
        "a moved goalpost must not prove against the real data"
    );
}
