//! Integration test (todo.md Phase 3): a deliberately-broken `#[zk_provable]`
//! lowering (an off-by-one in the circuit) is caught by verification with a
//! useful diagnostic, and a correct lowering passes.

use tpt_axiom_ir::{Constraint, ConstraintSystemBuilder};
use tpt_axiom_verify::{check_comparison, normalize_all, Comparison};

#[test]
fn correct_ge_lowering_passes() {
    // `assert!(sender_balance >= amount)` lowers to `NonNegative(sender - amount)`.
    let mut b = ConstraintSystemBuilder::new("transfer");
    let sender = b.public_input("sender_balance");
    let amount = b.secret_input("amount");
    let surplus = b.sub(sender, amount);
    b.constrain_non_negative(surplus);
    let cs = b.build();

    let table = normalize_all(&cs);
    let result = check_comparison(
        &table,
        &cs.constraints[0],
        Comparison::Ge,
        sender,
        amount,
    );
    assert!(result.is_ok(), "correct lowering should verify: {result:?}");
}

#[test]
fn off_by_one_lowering_is_caught() {
    // Deliberately broken: encodes `sender - amount - 1 >= 0` (i.e. `>`)
    // where the claimed comparison is `>=`. This is exactly the kind of
    // off-by-one lowering bug todo.md's Phase 3 milestone asks to catch.
    let mut b = ConstraintSystemBuilder::new("transfer_buggy");
    let sender = b.public_input("sender_balance");
    let amount = b.secret_input("amount");
    let diff = b.sub(sender, amount);
    let one = b.constant(1);
    let off_by_one = b.sub(diff, one);
    b.constrain_non_negative(off_by_one);
    let cs = b.build();

    let table = normalize_all(&cs);
    let result = check_comparison(
        &table,
        &cs.constraints[0],
        Comparison::Ge,
        sender,
        amount,
    );
    let mismatch = result.expect_err("off-by-one lowering must be rejected");
    let message = mismatch.to_string();
    assert!(
        message.contains("expected") && message.contains("found"),
        "diagnostic should be readable: {message}"
    );
}

#[test]
fn swapped_operands_are_caught() {
    // Deliberately broken: constrains `amount - sender >= 0` (backwards)
    // where the claimed comparison is `sender >= amount`.
    let mut b = ConstraintSystemBuilder::new("transfer_swapped");
    let sender = b.public_input("sender_balance");
    let amount = b.secret_input("amount");
    let backwards = b.sub(amount, sender);
    b.constrain_non_negative(backwards);
    let cs = b.build();

    let table = normalize_all(&cs);
    let result = check_comparison(
        &table,
        &cs.constraints[0],
        Comparison::Ge,
        sender,
        amount,
    );
    assert!(result.is_err(), "swapped operands must be rejected");
    assert!(matches!(cs.constraints[0], Constraint::NonNegative(_)));
}
