//! An A/B test decided with Bayesian posteriors — and honest uncertainty
//! about the decision itself.
//!
//! Two variants ran; each observed conversions out of visitors. The
//! conversion rates get conjugate-updated `Beta` posteriors
//! ([`Beta::update_bernoulli`]); the example reports credible intervals,
//! the posterior probability that B beats A, and applies an explicit
//! policy: ship B only when it is very likely better *and* the gain is
//! worth it. If the evidence is thin, the honest answer is "keep running
//! the test" — an abstention, not a guess.
//!
//! Run it (from the workspace root):
//!
//! ```sh
//! cargo run -p tpt-axiom --example ab_test
//! ```
//!
//! Expected output:
//!
//! ```text
//! variant A  : 128/1000 → posterior Beta(129, 873), mean 0.1287, 95% CI [0.109, 0.150]
//! variant B  : 156/1000 → posterior Beta(157, 845), mean 0.1567, 95% CI [0.135, 0.180]
//! P(B > A)  : 0.963 (grid quadrature over the two posteriors)
//! decision  : SHIP B — it is very likely better and the lift is material
//!
//! (thin evidence: 13/100 vs 15/100)
//! variant A' : 13/100 → posterior Beta(14, 88), mean 0.1373, 95% CI [0.078, 0.210]
//! variant B' : 15/100 → posterior Beta(16, 86), mean 0.1569, 95% CI [0.093, 0.233]
//! P(B' > A') : 0.656
//! decision   : KEEP RUNNING — P(B'>A') below the 0.95 bar
//! ```

#![allow(clippy::cast_precision_loss)] // u64 counts feed f64 moments

use tpt_axiom::prelude::*;
/// Posterior probability that variant B's rate exceeds variant A's, by
/// Simpson-rule quadrature over a grid (accurate to ~1e-5 for the smooth
/// posteriors an A/B test produces; the example says so, loudly).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn prob_b_beats_a(a: &Beta, b: &Beta) -> f64 {
    let steps = 400_usize;
    let h = 1.0 / steps as f64;
    let integrand = |p: f64| a.pdf(p) * b.prob_greater_than(p);
    let mut total = integrand(0.0) + integrand(1.0);
    for i in 1..steps {
        let x = i as f64 * h;
        total += if i % 2 == 1 { 4.0 } else { 2.0 } * integrand(x);
    }
    total * h / 3.0
}

fn report(name: &str, conversions: u64, visitors: u64, prior: &Beta) -> Beta {
    // Flat prior Beta(1, 1); the posterior is the conjugate update.
    let posterior = prior.update_bernoulli(conversions, visitors - conversions);
    let rate = conversions as f64 / visitors as f64;
    let ci = posterior.credible_interval(0.95);
    println!(
        "{name:<11}: {conversions}/{visitors} → posterior Beta({}, {}), mean {:.4}, 95% CI [{:.3}, {:.3}]",
        posterior.a(),
        posterior.b(),
        posterior.mean(),
        ci.0,
        ci.1
    );
    // Sanity: the flat prior's posterior mean sits near the observed rate.
    assert!((posterior.mean() - rate).abs() < 0.01);
    posterior
}

fn main() {
    let prior = Beta::new(1.0, 1.0).expect("flat prior");
    let ship_bar = 0.95; // P(B > A) required to ship
    let lift_bar = 0.02; // absolute conversion lift worth shipping

    // --- well-powered test -----------------------------------------------
    let a = report("variant A", 128, 1_000, &prior);
    let b = report("variant B", 156, 1_000, &prior);
    let p_beats = prob_b_beats_a(&a, &b);
    println!("P(B > A)  : {p_beats:.3} (grid quadrature over the two posteriors)");

    let lift = b.mean() - a.mean();
    let decision = if p_beats >= ship_bar && lift >= lift_bar {
        "SHIP B — it is very likely better and the lift is material"
    } else if p_beats >= ship_bar {
        "SHIP B — likely better, watch the lift"
    } else {
        "KEEP RUNNING — P(B > A) below the 0.95 bar"
    };
    println!("decision  : {decision}");
    assert!(
        p_beats >= ship_bar,
        "128/1000 vs 156/1000 must clear the bar"
    );

    // --- thin evidence ------------------------------------------------------
    println!("\n(thin evidence: 13/100 vs 15/100)");
    let a2 = report("variant A'", 13, 100, &prior);
    let b2 = report("variant B'", 15, 100, &prior);
    let p2 = prob_b_beats_a(&a2, &b2);
    println!("P(B' > A') : {p2:.3}");
    let decision2 = if p2 >= ship_bar {
        "SHIP B'"
    } else {
        "KEEP RUNNING — P(B'>A') below the 0.95 bar"
    };
    println!("decision   : {decision2}");
    assert!(p2 < ship_bar, "thin evidence must not ship");
}
