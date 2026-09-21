//! Checking a [`ConstraintSystem`] against the comparison it should encode.
//!
//! `#[zk_provable]`'s macro (`tpt-axiom-macros`) lowers each `assert!`
//! comparison into a [`Constraint`] over the expression DAG, but the
//! [`ConstraintSystem`] itself only remembers the *result*
//! (`NonNegative(e)`/`Equal(l, r)`), not which original comparison produced
//! it. To catch a lowering bug (e.g. an off-by-one turning `>` into `>=`,
//! or the operands swapped), the caller supplies what the comparison was
//! supposed to mean — [`Comparison`] — and [`check_comparison`] proves, by
//! exact polynomial identity, that the constraint the system actually
//! contains matches it.

use alloc::vec::Vec;

use tpt_axiom_ir::circuit::ExprId;
use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr, Scalar};

use crate::poly::{expect_equal, Mismatch, Polynomial};

/// Which comparison a [`Constraint`] is supposed to encode. Mirrors the
/// operators `tpt-axiom-macros`'s `lower_comparison` recognizes (see
/// `crates/tpt-axiom-macros/src/lower.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// `lhs >= rhs`
    Ge,
    /// `lhs <= rhs`
    Le,
    /// `lhs > rhs`
    Gt,
    /// `lhs < rhs`
    Lt,
    /// `lhs == rhs`
    Eq,
}

/// Computes the canonical polynomial for every expression in `cs`, indexed by
/// [`ExprId`]. `#[zk_provable]`'s builder (`ConstraintSystemBuilder`) only
/// ever pushes an expression after the sub-expressions it references, so a
/// single forward pass is enough — no recursion needed.
#[must_use]
pub fn normalize_all(cs: &ConstraintSystem) -> Vec<Polynomial> {
    let mut table: Vec<Polynomial> = Vec::with_capacity(cs.exprs.len());
    for expr in &cs.exprs {
        let p = match *expr {
            Expr::Const(v) => Polynomial::constant(v),
            Expr::Var(id) => Polynomial::variable(id),
            Expr::Add(l, r) => table[l].clone() + table[r].clone(),
            Expr::Sub(l, r) => table[l].clone() - table[r].clone(),
            Expr::Mul(l, r) => table[l].clone() * table[r].clone(),
            Expr::Neg(e) => -table[e].clone(),
        };
        table.push(p);
    }
    table
}

/// Proves (or disproves) that `constraint` encodes `lhs <cmp> rhs`.
///
/// `table` must be [`normalize_all`]'s output for the same `cs`.
///
/// # Errors
///
/// Returns [`Mismatch`] if the constraint's polynomial does not match what
/// the claimed comparison requires.
pub fn check_comparison(
    table: &[Polynomial],
    constraint: &Constraint,
    cmp: Comparison,
    lhs: ExprId,
    rhs: ExprId,
) -> Result<(), Mismatch> {
    let one = Polynomial::constant(1);
    let expected = match cmp {
        Comparison::Ge | Comparison::Eq => table[lhs].clone() - table[rhs].clone(),
        Comparison::Le => table[rhs].clone() - table[lhs].clone(),
        Comparison::Gt => table[lhs].clone() - table[rhs].clone() - one,
        Comparison::Lt => table[rhs].clone() - table[lhs].clone() - one,
    };
    let found = match *constraint {
        Constraint::NonNegative(e) | Constraint::Zero(e) => table[e].clone(),
        Constraint::Equal(l, r) => table[l].clone() - table[r].clone(),
    };
    expect_equal(found, expected)
}

/// Evaluates every expression in `cs` given concrete values for its named
/// variables, indexed by [`ExprId`] (same traversal order as
/// [`normalize_all`], but on concrete integers instead of symbols).
///
/// Used to cross-check the IR against the kept-original Rust function on
/// sampled inputs (see the crate-level integration tests). `inputs` need not
/// cover every named variable: a `#[zk_provable]` output slot (e.g.
/// `"return"`) is itself a free variable pinned by an `Equal` constraint
/// elsewhere, not something to feed a value into, so any variable missing
/// from `inputs` evaluates to `0` — callers read its real value from the
/// other side of that `Equal` constraint instead.
#[must_use]
pub fn evaluate(cs: &ConstraintSystem, inputs: &[(&str, Scalar)]) -> Vec<Scalar> {
    let lookup =
        |name: &str| -> Scalar { inputs.iter().find(|(n, _)| *n == name).map_or(0, |(_, v)| *v) };
    let mut table: Vec<Scalar> = Vec::with_capacity(cs.exprs.len());
    for expr in &cs.exprs {
        let v = match *expr {
            Expr::Const(c) => c,
            Expr::Var(id) => lookup(&cs.variables[id].name),
            Expr::Add(l, r) => table[l] + table[r],
            Expr::Sub(l, r) => table[l] - table[r],
            Expr::Mul(l, r) => table[l] * table[r],
            Expr::Neg(e) => -table[e],
        };
        table.push(v);
    }
    table
}
