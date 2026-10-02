//! End-to-end tests for the `#[zk_provable]` macro: the annotated function
//! still runs as ordinary Rust, and the generated circuit definition lowers it
//! to the backend-agnostic IR.

use tpt_axiom_ir::{Constraint, IntType};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_verify::circuit::evaluate;
use tpt_axiom_zk::CircuitDefinition;

#[zk_provable(backend = "halo2")]
fn narrow_types(#[public] small: u8, #[secret] delta: u8, #[public] offset: i16) {
    assert!(small >= 1);
    let combined = small + delta;
    assert!(combined >= delta);
    assert!(offset >= 0);
}

#[zk_provable(backend = "halo2")]
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

#[zk_provable(backend = "arkworks")]
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn weighted_sum(a: u64, b: u64, #[secret] k: u64) -> u64 {
    let scaled = a * k;
    scaled + b
}

#[zk_provable(backend = "sp1")]
fn bounds_check(#[secret] x: i64, #[public] lo: i64, #[public] hi: i64) {
    assert!(x >= lo);
    assert!(x <= hi);
    assert!(x < hi + 1);
}

#[test]
fn balance_transfer_lowers_to_ir() {
    let circuit = ProveBalanceTransfer;
    assert_eq!(circuit.name(), "prove_balance_transfer");
    assert_eq!(circuit.backend(), "halo2");

    let ir = circuit.build();
    assert_eq!(ir.name, "prove_balance_transfer");
    assert_eq!(ir.num_public(), 2);
    assert_eq!(ir.num_secret(), 1);
    assert_eq!(ir.constraints.len(), 2);
    assert!(matches!(ir.constraints[0], Constraint::NonNegative(_)));
    assert!(matches!(ir.constraints[1], Constraint::Equal(_, _)));
    assert!(ir.variable_id("sender_balance").is_some());
    assert!(ir.variable_id("amount").is_some());
}

#[test]
fn return_becomes_public_output() {
    let circuit = WeightedSum;
    assert_eq!(circuit.backend(), "arkworks");
    let ir = circuit.build();
    // a, b, and the returned output are public; k is secret.
    assert_eq!(ir.num_public(), 3);
    assert_eq!(ir.num_secret(), 1);
    assert_eq!(ir.constraints.len(), 1);
    assert!(matches!(ir.constraints[0], Constraint::Equal(_, _)));
}

#[test]
fn comparison_operators_lower() {
    let ir = BoundsCheck.build();
    assert_eq!(ir.num_public(), 2);
    assert_eq!(ir.num_secret(), 1);
    // x >= lo, x <= hi, x < hi+1 each become one non-negativity constraint.
    assert_eq!(ir.constraints.len(), 3);
    assert!(
        ir.constraints
            .iter()
            .all(|c| matches!(c, Constraint::NonNegative(_)))
    );
}

#[zk_provable(backend = "halo2")]
fn conjunction(#[secret] x: i64, #[public] lo: i64, #[public] hi: i64) {
    assert!(x >= lo && x <= hi);
}

#[test]
fn conjunction_of_comparisons_lowers_both_halves() {
    let ir = Conjunction.build();
    // `x >= lo && x <= hi` — one constraint per comparison, in order.
    assert_eq!(ir.constraints.len(), 2);
    assert!(
        ir.constraints
            .iter()
            .all(|c| matches!(c, Constraint::NonNegative(_)))
    );
    assert!(ir.validate().is_ok());
    // Semantics: the lowered constraints mean exactly the conjunction.
    let x = ir.variable_id("x").expect("x");
    let lo = ir.variable_id("lo").expect("lo");
    let hi = ir.variable_id("hi").expect("hi");
    let table = tpt_axiom_verify::circuit::normalize_all(&ir);
    tpt_axiom_verify::circuit::check_comparison(
        &table,
        &ir.constraints[0],
        tpt_axiom_verify::Comparison::Ge,
        x,
        lo,
    )
    .expect("first half must encode x >= lo");
    tpt_axiom_verify::circuit::check_comparison(
        &table,
        &ir.constraints[1],
        tpt_axiom_verify::Comparison::Le,
        x,
        hi,
    )
    .expect("second half must encode x <= hi");
}

#[zk_provable(backend = "arkworks")]
/// Division and remainder lower to the quotient/remainder gadget for
/// unsigned operands; the original function still runs as plain Rust.
fn unsigned_division(x: u64, d: u64) -> u64 {
    let q = x / d;
    let r = x % d;
    assert!(r < d);
    q
}

#[test]
fn division_gadget_lowers_and_reruns() {
    // Original Rust semantics kept.
    assert_eq!(unsigned_division(100, 7), 14);
    let ir = UnsignedDivision.build();
    // x, q, r... the assert's NonNegative plus the gadget's two NonNegative
    // range checks (remainder >= 0, divisor - remainder - 1 >= 0).
    assert!(ir.constraints.len() >= 3);
    assert!(ir.validate().is_ok());
}

#[zk_provable(backend = "halo2")]
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
#[allow(clippy::eq_op)] // the point is that debug_assert_eq! lowers at all
fn debug_assertions(#[secret] x: i64, #[public] hi: i64) {
    debug_assert!(x <= hi);
    debug_assert_eq!(x, x);
}

#[test]
fn debug_assertions_are_always_enforced() {
    // A circuit has no debug builds: debug_assert! must lower to the same
    // constraints assert! would.
    let ir = DebugAssertions.build();
    assert_eq!(ir.constraints.len(), 2);
    assert!(matches!(ir.constraints[0], Constraint::NonNegative(_)));
    assert!(matches!(ir.constraints[1], Constraint::Equal(_, _)));
    assert!(ir.validate().is_ok());
}

#[test]
fn original_functions_still_run_as_plain_rust() {
    // The macro keeps the original function as the source-of-truth Rust logic.
    prove_balance_transfer(50, 20, 30);
    assert_eq!(weighted_sum(2, 3, 4), 11);
    bounds_check(5, 0, 10);
}

#[test]
fn public_and_secret_classification() {
    let ir = ProveBalanceTransfer.build();
    let public: Vec<&str> = ir
        .public_inputs
        .iter()
        .filter_map(|&id| match &ir.exprs[id] {
            tpt_axiom_ir::Expr::Var(v) => ir.variables.get(*v).map(|info| info.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(public, vec!["sender_balance", "receiver_balance"]);
    let secret: Vec<&str> = ir
        .secret_inputs
        .iter()
        .filter_map(|&id| match &ir.exprs[id] {
            tpt_axiom_ir::Expr::Var(v) => ir.variables.get(*v).map(|info| info.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(secret, vec!["amount"]);
}

#[test]
fn declared_parameter_types_reach_the_ir() {
    let ir = NarrowTypes.build();
    let small = ir.variable_id("small").expect("small declared");
    let delta = ir.variable_id("delta").expect("delta declared");
    let offset = ir.variable_id("offset").expect("offset declared");
    // The declared widths survive lowering, so backends can range check each
    // parameter against its own type instead of a blanket signed i64.
    assert_eq!(ir.expr_int_type(small), Some(IntType::U8));
    assert_eq!(ir.expr_int_type(delta), Some(IntType::U8));
    assert_eq!(ir.expr_int_type(offset), Some(IntType::I16));
}

#[test]
fn u64_parameters_are_recorded_as_unsigned() {
    let ir = ProveBalanceTransfer.build();
    let sender = ir.variable_id("sender_balance").expect("sender");
    assert_eq!(ir.expr_int_type(sender), Some(IntType::U64));
}

// --- macro-generated typed inputs (todo.md Phase C: typed input structs,
// --- name-keyed witness API, public-input layout, typed prove/verify)

#[test]
fn typed_inputs_carry_each_parameter_declared_type() {
    let inputs = NarrowTypesInputs::new(7, 3, -5);
    // Field types are the parameters' own types, so a wrong-typed value is a
    // compile error rather than a runtime surprise.
    let _: u8 = inputs.small;
    let _: u8 = inputs.delta;
    let _: i16 = inputs.offset;
    assert_eq!(NarrowTypesInputs::public_names(), ["small", "offset"]);
    assert_eq!(NarrowTypesInputs::secret_names(), ["delta"]);
}

#[test]
fn typed_inputs_resolve_onto_the_circuit_in_declaration_order() {
    let inputs = ProveBalanceTransferInputs::new(50, 20, 30);
    let witness = inputs.named().expect("values fit the IR's scalar model");
    let ir = ProveBalanceTransfer.build();
    let values = witness.resolve(&ir).expect("witness names every input");
    assert_eq!(values.public(), &[50, 20]);
    assert_eq!(values.secret(), &[30]);
    assert!(values.check(&ir).is_ok());
}

#[test]
fn field_order_in_the_literal_is_irrelevant() {
    // The whole point of keying by name: reading two u64 fields in the wrong
    // order cannot silently restate the claim.
    let ordered = ProveBalanceTransferInputs::new(50, 20, 30);
    let mut shuffled = ProveBalanceTransferInputs::new(20, 50, 30);
    shuffled.sender_balance = ordered.sender_balance;
    shuffled.receiver_balance = ordered.receiver_balance;
    let ir = ProveBalanceTransfer.build();
    let a = ordered.named().expect("fits").resolve(&ir).expect("named");
    let b = shuffled.named().expect("fits").resolve(&ir).expect("named");
    assert_eq!(a, b);
}

#[test]
fn a_constraint_violating_input_set_is_rejected_by_name() {
    // `amount` above `sender_balance` must not resolve into a provable witness.
    let inputs = ProveBalanceTransferInputs::new(10, 20, 30);
    let ir = ProveBalanceTransfer.build();
    assert!(
        inputs
            .named()
            .expect("values fit")
            .resolve_and_check(&ir)
            .is_err()
    );
}

#[test]
fn an_unsigned_value_above_the_scalar_model_is_named_not_cast() {
    // The IR models scalars as i64; a u64 above i64::MAX must be refused
    // rather than wrapped into a negative number that proves something else.
    let inputs = ProveBalanceTransferInputs::new(u64::MAX, 0, 1);
    assert_eq!(
        inputs.named().unwrap_err(),
        tpt_axiom_zk::NamedWitnessError::ScalarOutOfRange {
            name: "sender_balance"
        }
    );
    // A u64 inside the model converts fine.
    let ok = ProveBalanceTransferInputs::new(1 << 40, 0, 1);
    assert_eq!(
        ok.named().expect("1 << 40 fits i64").get("sender_balance"),
        Some(1 << 40)
    );
}

#[test]
fn a_returning_circuit_binds_its_output_explicitly() {
    let inputs = WeightedSumInputs::new(2, 3, 4);
    // The output slot is public but computed, so `named()` alone cannot resolve.
    let ir = WeightedSum.build();
    let without_output = inputs.named().expect("values fit");
    assert_eq!(
        without_output.resolve(&ir),
        Err(tpt_axiom_zk::NamedWitnessError::MissingInput {
            name: "return".to_owned(),
            kind: "public",
            index: 2,
        })
    );
    let with_output = WeightedSumInputs::with_output(without_output, 11);
    let values = with_output
        .resolve_and_check(&ir)
        .expect("complete witness");
    assert_eq!(values.public(), &[2, 3, 11]);
    assert_eq!(values.secret(), &[4]);
}

#[test]
fn the_layout_printout_lists_what_a_verifier_sees() {
    let ir = WeightedSum.build();
    let layout = tpt_axiom_zk::InputLayout::of(&ir);
    let names: Vec<&str> = layout
        .public_slots()
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["a", "b", "return"]);
    assert!(layout.public_slots()[2].is_output);
    let printed = layout.to_string();
    assert!(printed.contains("return (output)"), "printed: {printed}");
    assert!(printed.contains("secret #0  k"), "printed: {printed}");
    // The generated names agree with the IR's own classification.
    assert_eq!(WeightedSumInputs::public_names(), ["a", "b"]);
    assert_eq!(WeightedSumInputs::secret_names(), ["k"]);
}

#[test]
fn generated_inputs_are_copy_and_comparable() {
    // Needed so a caller can hold several candidate input sets at once.
    let a = BoundsCheckInputs::new(5, 0, 10);
    let b = a;
    assert_eq!(a, b);
}

// Cross-checks the IR against the kept-original Rust function on sampled
// inputs, using `tpt-axiom-verify`'s concrete evaluator. This is the
// practical stand-in for "a deliberately-broken circuit is caught" from
// todo.md's Phase 3 milestone: run the same style of comparison against a
// hand-rolled broken `ConstraintSystem` in `tpt-axiom-verify`'s own
// `tests/circuit_mismatch.rs`.
proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    #[test]
    #[allow(clippy::cast_possible_wrap)] // u64 samples feed i64 circuits
    fn weighted_sum_ir_matches_rust(a in 0u64..1000, b in 0u64..1000, k in 0u64..1000) {
        let expected = weighted_sum(a, b, k);
        let ir = WeightedSum.build();
        // The "return" variable is itself a free output slot, not something
        // to feed a value into; its value is pinned by the `Equal(out, ...)`
        // constraint the macro emits for `bind_output`, so evaluate the
        // constraint's other side instead of the output variable itself.
        let out_id = ir.variable_id("return").expect("return output declared");
        let value_id = ir
            .constraints
            .iter()
            .find_map(|c| match *c {
                Constraint::Equal(l, r) if l == out_id => Some(r),
                Constraint::Equal(l, r) if r == out_id => Some(l),
                _ => None,
            })
            .expect("output equality constraint not found");
        let table = evaluate(&ir, &[("a", a as i64), ("b", b as i64), ("k", k as i64)]);
        proptest::prop_assert_eq!(table[value_id], expected as i64);
    }

    #[test]
    fn bounds_check_ir_matches_rust(x in -1000i64..1000, lo in -1000i64..1000, hi in -1000i64..1000) {
        // `bounds_check` panics via `assert!` for most sampled inputs (the
        // range is wider than the valid `[lo, hi]` window on purpose, to
        // exercise both the pass and fail paths); silence the default panic
        // hook so the expected panics don't spam test output.
        static SILENCE_PANICS: std::sync::Once = std::sync::Once::new();
        SILENCE_PANICS.call_once(|| std::panic::set_hook(Box::new(|_| {})));
        let rust_ok = std::panic::catch_unwind(|| bounds_check(x, lo, hi)).is_ok();
        let ir = BoundsCheck.build();
        let table = evaluate(&ir, &[("x", x), ("lo", lo), ("hi", hi)]);
        let ir_ok = ir.constraints.iter().all(|c| match *c {
            Constraint::NonNegative(e) => table[e] >= 0,
            Constraint::Zero(e) => table[e] == 0,
            Constraint::Equal(l, r) => table[l] == table[r],
        });
        proptest::prop_assert_eq!(rust_ok, ir_ok);
    }
}

// --- Gadgets: `!=`, `if`, bounded `for` -------------------------------

#[zk_provable(backend = "halo2")]
#[allow(clippy::needless_if)] // the `if` statement is the point
fn not_equal(#[public] a: i64, #[secret] b: i64) {
    assert!(a != b);
}

#[test]
fn not_equal_lowers_to_a_free_selector_gadget() {
    let ir = NotEqual.build();
    assert!(ir.validate().is_ok());
    // The gadget's selector is a free witness: secret-visibility, never a
    // named input, solved by the driver.
    assert_eq!(ir.num_free(), 1);
    assert_eq!(ir.num_secret(), 1);
    // Booleanity + two disjunction arms.
    assert!(ir.constraints.len() >= 3);
    // The kept original still runs as plain Rust.
    not_equal(3, 4);
}

#[zk_provable(backend = "halo2")]
#[allow(clippy::needless_if)] // the `if` statement is the point
fn gated_overdraft(#[public] limit: i64, #[secret] amount: i64, #[secret] balance: i64) {
    if amount >= limit {
        assert!(balance >= amount);
    }
}

#[test]
fn if_statement_lowers_to_gated_constraints() {
    let ir = GatedOverdraft.build();
    assert!(ir.validate().is_ok());
    assert_eq!(ir.num_free(), 1, "the branch selector");
    // The branch assert appears as a gated (multiplied) NonNegative, so
    // unselected branches are vacuous: at least one NonNegative targets a
    // Mul node (the selector gate times the branch difference).
    assert!(ir.constraints.iter().any(|c| matches!(
        c,
        Constraint::NonNegative(e) if matches!(ir.exprs[*e], tpt_axiom_ir::Expr::Mul(_, _))
    )));
    gated_overdraft(10, 3, 100);
}

#[zk_provable(backend = "arkworks")]
#[allow(clippy::let_and_return, clippy::bool_to_int_with_if)] // kept-fn shape is the point
#[allow(clippy::missing_const_for_fn)] // ...and fixed by the macro
fn threshold_classify(#[secret] score: i64, #[public] cutoff: i64) -> i64 {
    let label = if score >= cutoff { 1 } else { 0 };
    label
}

#[test]
fn if_expression_lowers_to_a_selector_multiplex() {
    // Kept-original semantics first.
    assert_eq!(threshold_classify(8, 5), 1);
    assert_eq!(threshold_classify(2, 5), 0);
    let ir = ThresholdClassify.build();
    assert!(ir.validate().is_ok());
    assert_eq!(ir.num_free(), 1);
}

#[zk_provable(backend = "arkworks")]
fn weighted_sum_unrolled(#[secret] x: i64, #[public] total: i64) {
    let mut acc = 0;
    for i in 1..5 {
        acc += i * x;
    }
    assert_eq!(acc, total);
}

#[test]
fn bounded_for_unrolls_with_an_accumulator() {
    // 1x + 2x + 3x + 4x = 10x; the kept original asserts internally.
    weighted_sum_unrolled(7, 70);
    let ir = WeightedSumUnrolled.build();
    assert!(ir.validate().is_ok());
    assert_eq!(ir.num_free(), 0, "loops need no selectors");
    // Four iterations, each contributing a Mul gate for `i * x`.
    let muls = ir
        .exprs
        .iter()
        .filter(|e| matches!(e, tpt_axiom_ir::Expr::Mul(_, _)))
        .count();
    assert!(muls >= 4);
}
