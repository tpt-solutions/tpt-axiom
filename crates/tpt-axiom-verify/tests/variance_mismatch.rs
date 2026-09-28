//! Integration test (todo.md Phase 3): a deliberately-wrong variance-
//! propagation formula is caught against the correct reference identity.

use tpt_axiom_verify::fuzzy::{check_mul_variance, MEAN_A, MEAN_B, VAR_A, VAR_B};
use tpt_axiom_verify::{expect_equal, Polynomial};

#[test]
fn correct_mul_variance_passes() {
    assert!(check_mul_variance().is_ok());
}

#[test]
fn dropped_term_is_caught() {
    // Deliberately wrong: drops the `Var(A)Var(B)` term from
    // `Var(AB) = Var(A)Var(B) + Var(A)E[B]^2 + Var(B)E[A]^2`.
    let broken = Polynomial::variable(VAR_A) * Polynomial::variable(MEAN_B) * Polynomial::variable(MEAN_B)
        + Polynomial::variable(VAR_B) * Polynomial::variable(MEAN_A) * Polynomial::variable(MEAN_A);
    let reference = Polynomial::variable(VAR_A) * Polynomial::variable(VAR_B)
        + Polynomial::variable(VAR_A) * Polynomial::variable(MEAN_B) * Polynomial::variable(MEAN_B)
        + Polynomial::variable(VAR_B) * Polynomial::variable(MEAN_A) * Polynomial::variable(MEAN_A);
    let mismatch = expect_equal(broken, reference).expect_err("dropped term must be rejected");
    let message = mismatch.to_string();
    assert!(
        message.contains("expected") && message.contains("found"),
        "diagnostic should be readable: {message}"
    );
}

#[test]
fn wrong_exponent_is_caught() {
    // Deliberately wrong: `k * variance` instead of `k^2 * variance`
    // (`Var(kX) = k Var(X)` instead of `k^2 Var(X)`).
    let k = Polynomial::variable(4);
    let var_a = Polynomial::variable(VAR_A);
    let broken = k.clone() * var_a.clone();
    let correct = (k.clone() * k) * var_a;
    assert!(expect_equal(broken, correct).is_err());
}
