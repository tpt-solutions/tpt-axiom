//! Backend-agnostic witness validation: check concrete public/secret values
//! against a circuit's IR constraints before any proving work.
//!
//! Real proving stacks do not evaluate constraints at prove time — a prover
//! handed a violating witness will happily produce a well-formed proof of a
//! false statement, which verifiers then reject. Checking the IR directly
//! (exact `i128` arithmetic, the same semantics `tpt-axiom-verify`
//! cross-checks against the original Rust function) rejects the witness up
//! front with a precise diagnosis instead.
//!
//! Two range rules sit on top of the raw constraints, mirroring what the
//! backends' bit-decomposition circuits can actually prove:
//!
//! * every named input must lie within its declared
//!   [`IntType`](tpt_axiom_ir::IntType);
//! * every `NonNegative` value must lie in `[0, 2^range_bits)` — the width of
//!   the backend's range-check chain (64 by default, see
//!   [`check_with_range`]).
//!
//! Free (aux) witness variables — the selector bits of disjunctive gadgets —
//! ride the *tail* of the secret slice, after the named secrets, in
//! declaration order. Callers do not produce them:
//! [`solve_free_variables`] searches the domain for a satisfying assignment
//! (the [`crate::driver`] calls it automatically), and the arity rule below
//! refuses a secret slice that is missing the tail.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr};

/// `NonNegative` values are provable in `[0, 2^range_bits)`; this is the
/// default chain width every backend uses unless configured otherwise.
pub const DEFAULT_RANGE_BITS: u32 = 64;

/// The most free (aux) witness variables [`solve_free_variables`] will search.
///
/// Each adds a factor of two; 16 keeps the worst case at 65 536 cheap exact
/// evaluations while leaving room for deeply nested conditional circuits.
pub const MAX_FREE_VARIABLES: usize = 16;

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
    /// Input `name` (public/secret slot `index`) carries `value`, outside
    /// its declared integer type's bounds.
    InputOutOfRange {
        /// The variable's declared name.
        name: String,
        /// Index into the public/secret input list.
        index: usize,
        /// The offending value.
        value: i128,
    },
    /// Constraint `index` requires a value in `[0, 2^bits)` (the backend's
    /// `NonNegative` range-check width) but carries `value`.
    NonNegativeOutOfRange {
        /// Index into `ConstraintSystem::constraints`.
        index: usize,
        /// The offending value.
        value: i128,
        /// The range-check width in bits.
        bits: u32,
    },
    /// Evaluating node `index` overflowed even `i128`; the circuit's
    /// semantics exceed the IR's scalar model entirely.
    Overflow {
        /// Index into `ConstraintSystem::exprs`.
        index: usize,
    },
    /// The circuit declares free (aux) witness variables and no assignment of
    /// them satisfies the constraints — for a `!=` gadget, that the two
    /// operands really are equal, so the claim is false.
    FreeVariablesUnsatisfiable {
        /// How many free variables were searched.
        count: usize,
    },
    /// The circuit declares more free (aux) witness variables than the solver
    /// will search ([`MAX_FREE_VARIABLES`]).
    TooManyFreeVariables {
        /// How many the circuit declares.
        count: usize,
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
            Self::InputOutOfRange { name, index, value } => write!(
                f,
                "input {name} (slot {index}) carries {value}, outside its declared integer type"
            ),
            Self::NonNegativeOutOfRange { index, value, bits } => write!(
                f,
                "constraint #{index} is provable only in [0, 2^{bits}) but its value is {value}"
            ),
            Self::Overflow { index } => {
                write!(
                    f,
                    "expression #{index} overflows the IR's i128 evaluation model"
                )
            }
            Self::FreeVariablesUnsatisfiable { count } => write!(
                f,
                "no assignment of the circuit's {count} free witness variable(s) satisfies the \
                 constraints (for a `!=` or `if` gadget this means the claim itself is false)"
            ),
            Self::TooManyFreeVariables { count } => write!(
                f,
                "the circuit declares {count} free witness variables; the solver searches at \
                 most {MAX_FREE_VARIABLES}"
            ),
        }
    }
}

impl core::error::Error for WitnessError {}

/// Validates a witness against the IR.
///
/// Checks correct arity, inputs within their declared types, and every
/// constraint satisfied under exact `i128` evaluation, with `NonNegative`
/// values limited to the default 64-bit range-check width.
///
/// Use [`check_with_range`] when the backend was configured with a narrower
/// range-check chain.
///
/// # Errors
/// Returns [`WitnessError::Arity`] on length mismatch,
/// [`WitnessError::InputOutOfRange`] for out-of-type inputs,
/// [`WitnessError::NonNegativeOutOfRange`] for a value past the range-check
/// width, [`WitnessError::Overflow`] when even `i128` overflows, and
/// [`WitnessError::Violated`] for the first unsatisfied constraint.
pub fn check(ir: &ConstraintSystem, public: &[i64], secret: &[i64]) -> Result<(), WitnessError> {
    check_with_range(ir, public, secret, DEFAULT_RANGE_BITS)
}

/// [`check`] with an explicit `NonNegative` range-check width.
///
/// Values must lie in `[0, 2^range_bits)` to be provable by the backend's
/// bit-decomposition chain (`range_bits` is capped at 64, matching the
/// backends' own decode).
///
/// # Errors
/// As [`check`].
pub fn check_with_range(
    ir: &ConstraintSystem,
    public: &[i64],
    secret: &[i64],
    range_bits: u32,
) -> Result<(), WitnessError> {
    if public.len() != ir.public_inputs.len() {
        return Err(WitnessError::Arity {
            kind: "public",
            expected: ir.public_inputs.len(),
            got: public.len(),
        });
    }
    // A circuit with free (aux) witness variables expects them appended after
    // the named secrets — see [`solve_free_variables`].
    let expected_secret = ir.secret_inputs.len() + ir.num_free();
    if secret.len() != expected_secret {
        return Err(WitnessError::Arity {
            kind: "secret",
            expected: expected_secret,
            got: secret.len(),
        });
    }
    let values = evaluate_checked(ir, Some(public), Some(secret))?;
    // Every named input must fit the integer type it was declared with — the
    // field-level circuits prove exactly that range, so a witness outside it
    // would fail proving with an opaque synthesis error instead of this
    // precise diagnosis.
    for (var, info) in ir.variables.iter().enumerate() {
        let Some(expr) = ir.var_expr_id(var) else {
            continue;
        };
        let Some(value) = values.get(expr).copied().flatten() else {
            continue;
        };
        let (min, max) = info.int_type.bounds();
        if value < min || value > max {
            let input_index = ir
                .public_inputs
                .iter()
                .chain(&ir.secret_inputs)
                .position(|&p| p == expr);
            return Err(WitnessError::InputOutOfRange {
                name: info.name.clone(),
                index: input_index.unwrap_or(var),
                value,
            });
        }
    }
    let range_bits = range_bits.min(64);
    for (index, constraint) in ir.constraints.iter().enumerate() {
        let satisfied = match *constraint {
            Constraint::Zero(e) => values.get(e).copied().flatten() == Some(0),
            Constraint::Equal(l, r) => {
                values.get(l).copied().flatten().is_some() && values.get(l) == values.get(r)
            }
            Constraint::NonNegative(e) => match values.get(e).copied().flatten() {
                Some(v) if v >= 0 => {
                    let limit = 1i128 << range_bits;
                    if v >= limit {
                        return Err(WitnessError::NonNegativeOutOfRange {
                            index,
                            value: v,
                            bits: range_bits,
                        });
                    }
                    true
                }
                _ => false,
            },
        };
        if !satisfied {
            return Err(WitnessError::Violated { index });
        }
    }
    Ok(())
}

/// Searches for an assignment of the circuit's free (aux) witness variables
/// that satisfies every constraint.
///
/// The result is in the tail-slice order the backends expect (appended after
/// the named secrets). Free variables are the selector bits of disjunctive
/// gadgets (`!=`, `if`);
/// they are not part of the caller-facing witness API. The search is
/// exhaustive over their domains — a `Bool` variable contributes the
/// candidates `0` and `1` — in ascending lexicographic order, so the result
/// is deterministic. `public` and `secret` hold exactly the *named* inputs.
///
/// # Errors
/// [`WitnessError::TooManyFreeVariables`] past [`MAX_FREE_VARIABLES`];
/// [`WitnessError::FreeVariablesUnsatisfiable`] when no assignment works
/// (which for a `!=` gadget means the claimed inequality is false); the
/// arity/overflow errors `check` reports when even the named values are
/// unusable.
pub fn solve_free_variables(
    ir: &ConstraintSystem,
    public: &[i64],
    secret: &[i64],
) -> Result<Vec<i64>, WitnessError> {
    let free = ir.free_variables();
    let count = free.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    if count > MAX_FREE_VARIABLES {
        return Err(WitnessError::TooManyFreeVariables { count });
    }
    // Ascending lexicographic search over the per-variable domains: candidate
    // `i` assigns bit `slot` of `i` to free variable `slot`. Every free
    // variable is currently a `Bool`; a future domain kind would append its
    // own candidates here.
    for search_index in 0..(1usize << count) {
        let candidate: Vec<i64> = (0..count)
            .map(|slot| i64::from((search_index >> slot) & 1 == 1))
            .collect();
        let mut extended = Vec::with_capacity(secret.len() + count);
        extended.extend_from_slice(secret);
        extended.extend_from_slice(&candidate);
        let values = evaluate_checked(ir, Some(public), Some(&extended))?;
        if all_constraints_satisfied(ir, &values, DEFAULT_RANGE_BITS) {
            return Ok(candidate);
        }
    }
    Err(WitnessError::FreeVariablesUnsatisfiable { count })
}

/// Whether every constraint is satisfied under `values`, with `NonNegative`
/// values limited to `[0, 2^range_bits)`. Unlike [`check_with_range`] this
/// collapses every failure into `false`: the solver only wants to know
/// whether a candidate assignment works.
fn all_constraints_satisfied(
    ir: &ConstraintSystem,
    values: &[Option<i128>],
    range_bits: u32,
) -> bool {
    let range_bits = range_bits.min(64);
    for constraint in &ir.constraints {
        let satisfied = match *constraint {
            Constraint::Zero(e) => values.get(e).copied().flatten() == Some(0),
            Constraint::Equal(l, r) => {
                values.get(l).copied().flatten().is_some() && values.get(l) == values.get(r)
            }
            Constraint::NonNegative(e) => match values.get(e).copied().flatten() {
                Some(v) if v >= 0 => v < (1i128 << range_bits),
                _ => false,
            },
        };
        if !satisfied {
            return false;
        }
    }
    true
}

/// Bottom-up exact `i128` evaluation of every expression node, given the
/// public and secret input values in declaration order.
///
/// Nodes whose evaluation overflows `i128` (and their dependents) evaluate to
/// `None`; use [`check`] when a precise [`WitnessError::Overflow`] diagnosis
/// is needed. This is the evaluator the backend adapters feed their witness
/// assignments from.
#[must_use]
pub fn evaluate_nodes(
    ir: &ConstraintSystem,
    public: Option<&[i64]>,
    secret: Option<&[i64]>,
) -> Vec<Option<i128>> {
    let mut values: Vec<Option<i128>> = Vec::with_capacity(ir.exprs.len());
    for expr in &ir.exprs {
        let value = match expr {
            Expr::Const(v) => Some(i128::from(*v)),
            Expr::Var(var) => variable_value(ir, *var, public, secret).map(i128::from),
            Expr::Add(l, r) => bin_op(&values, *l, *r, i128::checked_add),
            Expr::Sub(l, r) => bin_op(&values, *l, *r, i128::checked_sub),
            Expr::Mul(l, r) => bin_op(&values, *l, *r, i128::checked_mul),
            Expr::Div(l, r) => values
                .get(*l)
                .copied()
                .flatten()
                .zip(values.get(*r).copied().flatten())
                .filter(|(_, b)| *b != 0)
                .map(|(a, b)| a / b),
            Expr::Neg(n) => values
                .get(*n)
                .copied()
                .flatten()
                .and_then(i128::checked_neg),
        };
        values.push(value);
    }
    values
}

/// Exact evaluation that reports the first `i128` overflow as an error.
fn evaluate_checked(
    ir: &ConstraintSystem,
    public: Option<&[i64]>,
    secret: Option<&[i64]>,
) -> Result<Vec<Option<i128>>, WitnessError> {
    let mut values: Vec<Option<i128>> = Vec::with_capacity(ir.exprs.len());
    for (id, expr) in ir.exprs.iter().enumerate() {
        let value = match expr {
            Expr::Const(v) => Some(i128::from(*v)),
            Expr::Var(var) => variable_value(ir, *var, public, secret).map(i128::from),
            Expr::Add(l, r) => binary(&values, *l, *r, i128::checked_add, id)?,
            Expr::Sub(l, r) => binary(&values, *l, *r, i128::checked_sub, id)?,
            Expr::Mul(l, r) => binary(&values, *l, *r, i128::checked_mul, id)?,
            Expr::Div(l, r) => div_trunc(&values, *l, *r),
            Expr::Neg(n) => values
                .get(*n)
                .copied()
                .flatten()
                .and_then(i128::checked_neg),
        };
        values.push(value);
    }
    Ok(values)
}

fn variable_value(
    ir: &ConstraintSystem,
    var: usize,
    public: Option<&[i64]>,
    secret: Option<&[i64]>,
) -> Option<i64> {
    let expr = ir.var_expr_id(var);
    let public_index = expr.and_then(|e| ir.public_inputs.iter().position(|&p| p == e));
    if let (Some(i), Some(public)) = (public_index, public) {
        return public.get(i).copied();
    }
    // Free (aux) variables read the secret slice's tail, after the named
    // secrets, in declaration order.
    if ir.variables.get(var).and_then(|info| info.aux).is_some() {
        let aux_index = ir.free_variables().iter().position(|&v| v == var)?;
        return secret.and_then(|s| s.get(ir.secret_inputs.len() + aux_index).copied());
    }
    let secret_index = expr.and_then(|e| ir.secret_inputs.iter().position(|&s| s == e));
    match (secret_index, secret) {
        (Some(i), Some(secret)) => secret.get(i).copied(),
        _ => None,
    }
}

/// Truncated integer division; division by zero evaluates to `None` — the
/// gadget's range constraints then fail, which is the documented behavior
/// for a zero divisor.
fn div_trunc(values: &[Option<i128>], l: usize, r: usize) -> Option<i128> {
    let (a, b) = (
        values.get(l).copied().flatten()?,
        values.get(r).copied().flatten()?,
    );
    if b == 0 {
        return None;
    }
    Some(a / b)
}

fn binary(
    values: &[Option<i128>],
    l: usize,
    r: usize,
    op: fn(i128, i128) -> Option<i128>,
    id: usize,
) -> Result<Option<i128>, WitnessError> {
    let (Some(l), Some(r)) = (
        values.get(l).copied().flatten(),
        values.get(r).copied().flatten(),
    ) else {
        return Ok(None);
    };
    op(l, r)
        .map(Some)
        .ok_or(WitnessError::Overflow { index: id })
}

fn bin_op(
    values: &[Option<i128>],
    l: usize,
    r: usize,
    op: fn(i128, i128) -> Option<i128>,
) -> Option<i128> {
    op(
        values.get(l).copied().flatten()?,
        values.get(r).copied().flatten()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_axiom_ir::{ConstraintSystemBuilder, IntType};

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

    #[test]
    fn evaluation_is_exact_not_wrapping() {
        // sender - amount = 50 - i64::MIN is a huge *positive* i128 value: it
        // satisfies NonNegative here (the backends' field circuits reject the
        // negative amount via the declared-type range check instead). The old
        // wrapping i64 evaluator reported this as a violation.
        let mut b = ConstraintSystemBuilder::new("exact");
        let sender = b.public_input_typed("sender", IntType::I64);
        let amount = b.secret_input_typed("amount", IntType::I64);
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let ir = b.build();
        assert!(check(&ir, &[50], &[i64::MIN]).is_ok());
        // The widest value a declared-i64 subtraction can produce,
        // i64::MAX - i64::MIN = 2^64 - 1, is exactly the last value the
        // default 64-bit range check admits.
        assert!(check(&ir, &[i64::MAX], &[i64::MIN]).is_ok());
    }

    #[test]
    fn non_negative_beyond_the_range_check_width_is_rejected() {
        // A product can exceed 2^64 while staying non-negative: not provable
        // by a 64-bit bit-decomposition chain, so the witness is rejected
        // with the range diagnosis rather than a generic violation.
        let mut b = ConstraintSystemBuilder::new("wide_nonneg");
        let x = b.public_input("x");
        let y = b.secret_input("y");
        let p = b.mul(x, y);
        b.constrain_non_negative(p);
        let ir = b.build();
        assert_eq!(
            check(&ir, &[1i64 << 40], &[1i64 << 30]),
            Err(WitnessError::NonNegativeOutOfRange {
                index: 0,
                value: 1i128 << 70,
                bits: 64,
            })
        );
    }

    #[test]
    fn inputs_must_fit_their_declared_type() {
        let mut b = ConstraintSystemBuilder::new("typed");
        b.public_input_typed("small", IntType::U8);
        let ir = b.build();
        let err = check(&ir, &[300], &[]).expect_err("300 is not a u8");
        assert_eq!(
            err,
            WitnessError::InputOutOfRange {
                name: String::from("small"),
                index: 0,
                value: 300,
            }
        );
        assert!(check(&ir, &[255], &[]).is_ok());
    }

    #[test]
    #[allow(clippy::many_single_char_names)] // x/y/z/w/v stand in for inputs
    fn overflow_is_reported_precisely() {
        // x * y * z * w * v with i64::MAX operands overflows even i128.
        let mut b = ConstraintSystemBuilder::new("boom");
        let x = b.public_input("x");
        let y = b.secret_input("y");
        let z = b.secret_input("z");
        let w = b.secret_input("w");
        let v = b.secret_input("v");
        let m1 = b.mul(x, y);
        let m2 = b.mul(m1, z);
        let m3 = b.mul(m2, w);
        let m4 = b.mul(m3, v);
        b.constrain_zero(m4);
        let ir = b.build();
        let big = i64::MAX;
        // (2^63-1)^2 still fits i128; the *second* product overflows, so the
        // diagnosis names m2.
        assert_eq!(
            check(&ir, &[big], &[big, big, big, big]),
            Err(WitnessError::Overflow { index: m2 })
        );
    }

    #[test]
    fn intermediate_beyond_i64_evaluates_exactly() {
        // A 100-bit intermediate: exact i128 evaluation keeps it honest, and
        // an Equal constraint against the same computation still holds.
        let mut b = ConstraintSystemBuilder::new("wide");
        let x = b.public_input("x");
        let y = b.secret_input("y");
        let z = b.secret_input("z");
        let p1 = b.mul(x, y);
        let p2 = b.mul(x, z);
        b.constrain_eq(p1, p2);
        let ir = b.build();
        let x = 1i64 << 40;
        let y = 1i64 << 40;
        let z = 1i64 << 40;
        assert!(check(&ir, &[x], &[y, z]).is_ok());
    }

    #[test]
    fn narrower_range_bits_tighten_the_non_negative_check() {
        let mut b = ConstraintSystemBuilder::new("narrow");
        let x = b.public_input("x");
        b.constrain_non_negative(x);
        let ir = b.build();
        assert!(check_with_range(&ir, &[1 << 40], &[], 64).is_ok());
        assert_eq!(
            check_with_range(&ir, &[1 << 40], &[], 32),
            Err(WitnessError::NonNegativeOutOfRange {
                index: 0,
                value: 1i128 << 40,
                bits: 32,
            })
        );
    }

    /// The IR shape the macro emits for `assert!(l != r)`: a free boolean
    /// selector `s`, booleanity, and the two gated range checks that force
    /// `s = 1 ⇒ l ≥ r + 1` and `s = 0 ⇒ l ≤ r − 1`.
    #[allow(clippy::many_single_char_names)] // l/r/d/s mirror the gadget algebra
    fn not_equal_ir() -> ConstraintSystem {
        use tpt_axiom_ir::IntType;
        let mut b = ConstraintSystemBuilder::new("not_equal");
        let l = b.public_input_typed("l", IntType::I64);
        let r = b.secret_input_typed("r", IntType::I64);
        let d = b.sub(l, r);
        let s = b.free_bool("__axiom_free0");
        let sd = b.mul(s, d);
        let ss = b.mul(s, s);
        b.constrain_eq(ss, s); // booleanity
        // s = 1 ⇒ d ≥ 1
        let arm1 = b.sub(sd, s);
        b.constrain_non_negative(arm1);
        // s = 0 ⇒ d ≤ −1: s·(d+1) − d − 1 ≥ 0
        let one = b.constant(1);
        let d1 = b.add(d, one);
        let sd1 = b.mul(s, d1);
        let arm2 = b.sub(sd1, d1);
        b.constrain_non_negative(arm2);
        b.build()
    }

    #[test]
    fn free_variables_solve_and_check() {
        let ir = not_equal_ir();
        // The named-only slice is refused: the aux tail is missing.
        assert_eq!(
            check(&ir, &[10], &[3]),
            Err(WitnessError::Arity {
                kind: "secret",
                expected: 2,
                got: 1,
            })
        );
        // The solver finds the selector (s = 1: 10 − 3 − 1 ≥ 0).
        let aux = solve_free_variables(&ir, &[10], &[3]).expect("satisfiable");
        assert_eq!(aux, [1]);
        // The tail-slice convention: aux values appended after the secrets.
        assert!(check(&ir, &[10], &[3, aux[0]]).is_ok());
    }

    #[test]
    fn free_variables_unsatisfiable_when_the_claim_is_false() {
        let ir = not_equal_ir();
        // l == r: neither selector value can satisfy the range checks.
        assert_eq!(
            solve_free_variables(&ir, &[7], &[7]),
            Err(WitnessError::FreeVariablesUnsatisfiable { count: 1 })
        );
    }

    #[test]
    fn solved_tail_flows_through_evaluation() {
        let ir = not_equal_ir();
        // Negative differences select s = 0 and are provable: the s = 0 arm
        // demands d ≤ −1 exactly.
        let aux = solve_free_variables(&ir, &[-5], &[9]).expect("satisfiable");
        assert_eq!(aux, [0]);
        assert!(check(&ir, &[-5], &[9, 0]).is_ok());
        // A out-of-domain aux value is rejected by the declared-type check
        // even before the constraints run.
        assert_eq!(
            check(&ir, &[10], &[3, 2]),
            Err(WitnessError::InputOutOfRange {
                name: String::from("__axiom_free0"),
                index: 2,
                value: 2,
            })
        );
    }
}
