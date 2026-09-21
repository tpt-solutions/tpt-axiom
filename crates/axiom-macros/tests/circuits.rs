//! End-to-end tests for the `#[zk_provable]` macro: the annotated function
//! still runs as ordinary Rust, and the generated circuit definition lowers it
//! to the backend-agnostic IR.

use axiom_ir::Constraint;
use axiom_macros::zk_provable;
use axiom_zk::CircuitDefinition;

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
fn weighted_sum(a: u64, b: u64, #[secret] k: u64) -> u64 {
    let scaled = a * k;
    let total = scaled + b;
    return total;
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
    assert!(ir
        .constraints
        .iter()
        .all(|c| matches!(c, Constraint::NonNegative(_))));
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
            axiom_ir::Expr::Var(v) => ir.variables.get(*v).map(|info| info.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(public, vec!["sender_balance", "receiver_balance"]);
    let secret: Vec<&str> = ir
        .secret_inputs
        .iter()
        .filter_map(|&id| match &ir.exprs[id] {
            axiom_ir::Expr::Var(v) => ir.variables.get(*v).map(|info| info.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(secret, vec!["amount"]);
}