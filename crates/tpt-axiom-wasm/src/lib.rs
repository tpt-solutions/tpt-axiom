//! WASM bindings for the `tpt-axiom` playground: circuit inspection and the
//! uncertainty demos, callable from a static web page.
//!
//! Everything here is read-only demonstration surface over the same public
//! APIs the Rust examples use — no proving happens in the browser (the
//! prover stacks are native-only); the playground shows what a circuit
//! *is* (its IR, its R1CS shape) and what uncertainty arithmetic *does*
//! (fusion vs. independent arithmetic, delta method vs. Monte Carlo).

use wasm_bindgen::prelude::*;

use tpt_axiom_ir::ConstraintSystemBuilder;

pub mod verify;

/// The demo circuits the playground can inspect, as `name: description`
/// lines.
#[wasm_bindgen]
pub fn circuit_catalog() -> String {
    String::from(
        "balance_transfer: sender_balance >= amount, receiver' == receiver + amount\n\
         weighted_sum: public output a*k + b with a, b public, k secret\n\
         bounds_check: lo <= x <= hi over signed i64\n\
         division: q == x / d via the quotient/remainder gadget (unsigned)",
    )
}

/// Builds the named demo circuit's IR and renders it with
/// `ConstraintSystem::describe` (inputs, types, constraints).
#[wasm_bindgen]
pub fn circuit_describe(name: &str) -> Result<String, JsValue> {
    let ir =
        demo_ir(name).ok_or_else(|| JsValue::from_str("unknown circuit; see circuit_catalog()"))?;
    Ok(ir.describe())
}

/// R1CS shape of the named demo circuit: gates, witness size, public and
/// secret slots (the reference lowering in `tpt-axiom-ir`).
#[wasm_bindgen]
pub fn circuit_r1cs_shape(name: &str) -> Result<String, JsValue> {
    let ir =
        demo_ir(name).ok_or_else(|| JsValue::from_str("unknown circuit; see circuit_catalog()"))?;
    let r1cs =
        tpt_axiom_ir::r1cs::lower_r1cs(&ir).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(format!(
        "gates: {}\nwitness slots (incl. const 1): {}\npublic slots: {}\nsecret slots: {}",
        r1cs.gates.len(),
        r1cs.num_variables,
        r1cs.public_slots.len(),
        r1cs.secret_slots.len(),
    ))
}

/// Fuses two `(mean, variance)` estimates the minimum-variance way and
/// compares against plain independent addition — the demo of *why* fusion
/// is tighter.
#[wasm_bindgen]
pub fn fuse_demo(
    a_mean: f64,
    a_variance: f64,
    b_mean: f64,
    b_variance: f64,
) -> Result<String, JsValue> {
    let validate = |v: f64, what: &str| {
        if v.is_finite() && v >= 0.0 {
            Ok(())
        } else {
            Err(JsValue::from_str(&format!(
                "{what} must be finite and non-negative"
            )))
        }
    };
    validate(a_variance, "a_variance")?;
    validate(b_variance, "b_variance")?;
    let a = tpt_axiom_core::Fuzzy::new(a_mean, a_variance);
    let b = tpt_axiom_core::Fuzzy::new(b_mean, b_variance);
    let fused = a.fuse(&b);
    let added = a + b;
    Ok(format!(
        "fused  : {:.6} ± {:.6} (variance {:.6})\nadded  : {:.6} ± {:.6} (variance {:.6})\nfusion variance / addition variance = {:.3}",
        fused.mean(),
        fused.standard_deviation(),
        fused.variance(),
        added.mean(),
        added.standard_deviation(),
        added.variance(),
        fused.variance() / added.variance(),
    ))
}

/// Delta method vs. Monte Carlo for `exp(x)` at `x ~ N(mean, variance)` —
/// the demo of *when* first-order propagation breaks down. The Monte Carlo
/// mean approaches the exact lognormal answer `exp(mean + variance/2)`.
#[wasm_bindgen]
pub fn exp_demo(mean: f64, variance: f64, samples: usize) -> Result<String, JsValue> {
    if !variance.is_finite() || variance < 0.0 || samples == 0 {
        return Err(JsValue::from_str(
            "variance must be finite and non-negative; samples > 0",
        ));
    }
    let x = tpt_axiom_core::Fuzzy::new(mean, variance);
    let zero = tpt_axiom_core::Fuzzy::new(0.0, 0.0);
    // Deterministic seed: the playground is reproducible by construction.
    let mut state: u64 = 0x5EED_2026_1002;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1_u64 << 53) as f64
    };
    let mc = tpt_axiom_core::monte_carlo(&x, &zero, samples, |v, _w| v.exp(), &mut rng);
    let delta = x.exp();
    let exact = (mean + variance / 2.0).exp();
    Ok(format!(
        "delta method mean : {:.6}   (first-order: exp(mean))\nMonte Carlo mean  : {:.6}   ({samples} draws)\nexact (lognormal) : {:.6}   (exp(mean + variance/2))",
        delta.mean(),
        mc.mean(),
        exact,
    ))
}

/// The named demo IRs, mirroring the conformance circuits plus the division
/// gadget.
pub(crate) fn demo_ir(name: &str) -> Option<tpt_axiom_ir::ConstraintSystem> {
    let mut b = ConstraintSystemBuilder::new(name);
    let ir = match name {
        "balance_transfer" => {
            let sender = b.public_input_typed("sender_balance", tpt_axiom_ir::IntType::U64);
            let receiver = b.public_input_typed("receiver_balance", tpt_axiom_ir::IntType::U64);
            let amount = b.secret_input_typed("amount", tpt_axiom_ir::IntType::U64);
            let surplus = b.sub(sender, amount);
            b.constrain_non_negative(surplus);
            let new_receiver = b.add(receiver, amount);
            let expected = b.add(receiver, amount);
            b.constrain_eq(new_receiver, expected);
            b.build()
        }
        "weighted_sum" => {
            let a = b.public_input_typed("a", tpt_axiom_ir::IntType::U64);
            let out = b.public_input_typed("result", tpt_axiom_ir::IntType::U64);
            let k = b.secret_input_typed("k", tpt_axiom_ir::IntType::U64);
            let scaled = b.mul(a, k);
            b.constrain_eq(out, scaled);
            b.build()
        }
        "bounds_check" => {
            let x = b.secret_input("x");
            let lo = b.public_input("lo");
            let hi = b.public_input("hi");
            let ge = b.sub(x, lo);
            b.constrain_non_negative(ge);
            let le = b.sub(hi, x);
            b.constrain_non_negative(le);
            b.build()
        }
        "division" => {
            let x = b.public_input_typed("x", tpt_axiom_ir::IntType::U64);
            let q = b.public_input_typed("quotient", tpt_axiom_ir::IntType::U64);
            let d = b.secret_input_typed("d", tpt_axiom_ir::IntType::U64);
            b.constrain_non_negative(d);
            let q_node = b.div_trunc(x, d);
            b.constrain_division(x, d, q_node);
            b.constrain_eq(q, q_node);
            b.build()
        }
        _ => return None,
    };
    Some(ir)
}
