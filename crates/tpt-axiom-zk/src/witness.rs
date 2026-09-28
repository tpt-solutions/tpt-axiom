//! Backend-agnostic witness validation: check concrete public/secret values
//! against a circuit's IR constraints before any proving work.
//!
//! Real proving stacks do not evaluate constraints at prove time — a prover
//! handed a violating witness will happily produce a well-formed proof of a
//! false statement, which verifiers then reject. Checking the IR directly
//! (the same `i64` semantics `tpt-axiom-verify` cross-checks against the
//! original Rust function) rejects the witness up front with a precise
//! diagnosis instead.

use alloc::vec::Vec;

use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr};

/// Witness-shape or constraint-satisfaction failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessError {
    /// Public or secret slice length mismatch.
    Arity {
        /// Which slice mismatched.
        kind: &'static str,
        /// Expected number of values.
        expected: usize,
        /// Provided number of values.
        got: usize,
    },
    /// The witness violates the circuit's `index`-th constraint.
    Violated {
        /// Index into `ConstraintSystem::constraints`.
        index: usize,
    },
}

impl core::fmt::Display for WitnessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Arity {
                kind,
                expected,
                got,
            } => write!(
                f,
                "{kind} witness has {got} values but the circuit declares {expected}"
            ),
            Self::Violated { index } => {
                write!(f, "witness violates the circuit's constraint #{index}")
            }
        }
    }
}

/// Validates a witness against the IR: correct arity, and every constraint
/// satisfied under `i64` evaluation (wrapping, mirroring the node values the
/// circuit lowering itself will assign).
///
/// # Errors
/// Returns [`WitnessError::Arity`] on length mismatch and
/// [`WitnessError::Violated`] for the first unsatisfied constraint.
pub fn check(ir: &ConstraintSystem, public: &[i64], secret: &[i64]) -> Result<(), WitnessError> {
    if public.len() != ir.public_inputs.len() {
        return Err(WitnessError::Arity {
            kind: "public",
            expected: ir.public_inputs.len(),
            got: public.len(),
        });
    }
    if secret.len() != ir.secret_inputs.len() {
        return Err(WitnessError::Arity {
            kind: "secret",
            expected: ir.secret_inputs.len(),
            got: secret.len(),
        });
    }
    let values = evaluate_nodes(ir, Some(public), Some(secret));
    for (index, constraint) in ir.constraints.iter().enumerate() {
        let satisfied = match *constraint {
            Constraint::Zero(e) => values.get(e).copied().flatten() == Some(0),
            Constraint::Equal(l, r) => {
                values.get(l).copied().flatten().is_some() && values.get(l) == values.get(r)
            }
            Constraint::NonNegative(e) => values.get(e).copied().flatten().is_some_and(|v| v >= 0),
        };
        if !satisfied {
            return Err(WitnessError::Violated { index });
        }
    }
    Ok(())
}

/// Bottom-up `i64` (wrapping) evaluation of every expression node, given the
/// public and secret input values in declaration order.
#[must_use]
pub fn evaluate_nodes(
    ir: &ConstraintSystem,
    public: Option<&[i64]>,
    secret: Option<&[i64]>,
) -> Vec<Option<i64>> {
    let mut values: Vec<Option<i64>> = Vec::with_capacity(ir.exprs.len());
    for expr in &ir.exprs {
        let value = match expr {
            Expr::Const(v) => Some(*v),
            Expr::Var(var) => variable_value(ir, *var, public, secret),
            Expr::Add(l, r) => binary(&values, *l, *r, i64::wrapping_add),
            Expr::Sub(l, r) => binary(&values, *l, *r, i64::wrapping_sub),
            Expr::Mul(l, r) => binary(&values, *l, *r, i64::wrapping_mul),
            Expr::Neg(n) => values.get(*n).copied().flatten().map(i64::wrapping_neg),
        };
        values.push(value);
    }
    values
}

fn variable_value(
    ir: &ConstraintSystem,
    var: usize,
    public: Option<&[i64]>,
    secret: Option<&[i64]>,
) -> Option<i64> {
    let public_index = ir
        .public_inputs
        .iter()
        .position(|&p| matches!(ir.exprs.get(p), Some(Expr::Var(pv)) if *pv == var));
    if let (Some(i), Some(public)) = (public_index, public) {
        return public.get(i).copied();
    }
    let secret_index = ir
        .secret_inputs
        .iter()
        .position(|&s| matches!(ir.exprs.get(s), Some(Expr::Var(sv)) if *sv == var));
    match (secret_index, secret) {
        (Some(i), Some(secret)) => secret.get(i).copied(),
        _ => None,
    }
}

fn binary(values: &[Option<i64>], l: usize, r: usize, op: fn(i64, i64) -> i64) -> Option<i64> {
    Some(op(
        values.get(l).copied().flatten()?,
        values.get(r).copied().flatten()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_axiom_ir::ConstraintSystemBuilder;

    #[test]
    fn accepts_and_rejects_balance_transfer() {
        let mut b = ConstraintSystemBuilder::new("t");
        let sender = b.public_input("sender");
        let receiver = b.public_input("receiver");
        let amount = b.secret_input("amount");
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let new_receiver = b.add(receiver, amount);
        let expected = b.add(receiver, amount);
        b.constrain_eq(new_receiver, expected);
        let ir = b.build();

        assert!(check(&ir, &[50, 20], &[30]).is_ok());
        assert_eq!(
            check(&ir, &[50, 20], &[100]),
            Err(WitnessError::Violated { index: 0 })
        );
        assert_eq!(
            check(&ir, &[50], &[30]),
            Err(WitnessError::Arity {
                kind: "public",
                expected: 2,
                got: 1,
            })
        );
    }
}
