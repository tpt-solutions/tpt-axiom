//! A concrete R1CS lowering of the IR.
//!
//! R1CS ("rank-1 constraint system") is the quadratic form
//! `Σ aᵢwᵢ · Σ bᵢwᵢ = Σ cᵢwᵢ` used by many proving systems. This module shows
//! the IR is capable of lowering to a classic R1CS instance; backend adapters
//! (Phase 4) compile natively into their proving system's constraint format
//! instead, but the semantics must agree with this reference.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::{Constraint, ConstraintSystem, Expr, Scalar};

/// A sparse linear combination `Σ cᵢ · witness[i]`.
pub type Linear = Vec<(usize, Scalar)>;

/// One R1CS gate: `eval(a) * eval(b) == eval(c)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct R1csGate {
    /// Left linear combination.
    pub a: Linear,
    /// Right linear combination.
    pub b: Linear,
    /// Output linear combination.
    pub c: Linear,
}

/// Post-quadratic assertions that have no field-level quadratic form without
/// extra machinery (bit-decomposition).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum R1csAssertion {
    /// `witness[var] >= 0` under integer evaluation; real backends encode this
    /// with range checks.
    NonNegative(usize),
}

/// A lowered R1CS circuit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct R1CS {
    /// Witness size including `witness[0] = 1`.
    pub num_variables: usize,
    /// The quadratic gates.
    pub gates: Vec<R1csGate>,
    /// Assertions checked during integer-space evaluation.
    pub assertions: Vec<R1csAssertion>,
    /// Witness slot holding each public input (index = order they were added).
    pub public_slots: Vec<usize>,
    /// Witness slot holding each secret input.
    pub secret_slots: Vec<usize>,
    /// Witness slot holding each expression node (index = ExprId).
    pub expr_slots: Vec<Option<usize>>,
    /// Witness slot holding each named variable (index = variable id).
    pub var_slots: Vec<Option<usize>>,
    /// Variable names for diagnostics; index = witness slot.
    pub names: Vec<Option<String>>,
}

fn normalize(mut lin: Linear) -> Linear {
    lin.sort_by_key(|&(i, _)| i);
    lin.dedup_by(|a, b| {
        if a.0 == b.0 {
            b.1 += a.1;
            true
        } else {
            false
        }
    });
    lin.retain(|&(_, c)| c != 0);
    lin
}

fn const_gate(slot: usize, value: Scalar) -> R1csGate {
    R1csGate {
        a: vec![(slot, 1)],
        b: vec![(0, 1)],
        c: vec![(0, value)],
    }
}

/// Lower a [`ConstraintSystem`] into R1CS.
///
/// Every expression node gets a witness slot. Multiplication becomes a genuine
/// quadratic gate; additions, subtractions, negations, equality and zero
/// constraints are encoded as `A · 1 = C` gates (a standard trick), and
/// constants are pinned to their value with a dedicated gate. The constant
/// `witness[0]` is always `1`.
pub fn lower_r1cs(ir: &ConstraintSystem) -> R1CS {
    let mut gates = Vec::new();
    let mut assertions = Vec::new();

    // Allocate a witness slot for every expression node (witness[0] === 1).
    let mut expr_slots: Vec<Option<usize>> = vec![None; ir.exprs.len()];
    let mut var_slots: Vec<Option<usize>> = vec![None; ir.variables.len()];
    let mut num_vars = 1;
    for (id, e) in ir.exprs.iter().enumerate() {
        expr_slots[id] = Some(num_vars);
        if let Expr::Var(v) = e {
            var_slots[*v] = Some(num_vars);
        }
        num_vars += 1;
    }

    let slot = |id: usize| -> usize { expr_slots[id].expect("unreachable: every expr has a slot") };

    for (id, e) in ir.exprs.iter().enumerate() {
        match e {
            Expr::Const(v) => gates.push(const_gate(slot(id), *v)),
            Expr::Var(_) => {}
            Expr::Add(l, r) => {
                let o = slot(id);
                gates.push(R1csGate {
                    a: normalize(vec![(o, 1), (slot(*l), -1), (slot(*r), -1)]),
                    b: const1(),
                    c: vec![],
                });
            }
            Expr::Sub(l, r) => {
                let o = slot(id);
                gates.push(R1csGate {
                    a: normalize(vec![(o, 1), (slot(*l), -1), (slot(*r), 1)]),
                    b: const1(),
                    c: vec![],
                });
            }
            Expr::Neg(n) => {
                let o = slot(id);
                gates.push(R1csGate {
                    a: normalize(vec![(o, 1), (slot(*n), 1)]),
                    b: const1(),
                    c: vec![],
                });
            }
            Expr::Mul(l, r) => {
                let o = slot(id);
                gates.push(R1csGate {
                    a: vec![(slot(*l), 1)],
                    b: vec![(slot(*r), 1)],
                    c: vec![(o, 1)],
                });
            }
        }
    }

    for c in &ir.constraints {
        match *c {
            Constraint::Zero(e) => {
                gates.push(R1csGate {
                    a: vec![(slot(e), 1)],
                    b: const1(),
                    c: vec![],
                });
            }
            Constraint::Equal(l, r) => {
                gates.push(R1csGate {
                    a: normalize(vec![(slot(l), 1), (slot(r), -1)]),
                    b: const1(),
                    c: vec![],
                });
            }
            Constraint::NonNegative(e) => {
                assertions.push(R1csAssertion::NonNegative(slot(e)));
            }
        }
    }

    let mut names: Vec<Option<String>> = vec![None; num_vars];
    names[0] = Some(String::from("1"));
    let mut public_slots = Vec::new();
    let mut secret_slots = Vec::new();
    for &pid in &ir.public_inputs {
        if let Some(s) = expr_slots[pid] {
            public_slots.push(s);
        }
    }
    for &sid in &ir.secret_inputs {
        if let Some(s) = expr_slots[sid] {
            secret_slots.push(s);
        }
    }
    for (vid, info) in ir.variables.iter().enumerate() {
        if let Some(s) = var_slots[vid] {
            names[s] = Some(info.name.clone());
        }
    }

    R1CS {
        num_variables: num_vars,
        gates,
        assertions,
        public_slots,
        secret_slots,
        expr_slots,
        var_slots,
        names,
    }
}

fn const1() -> Linear {
    vec![(0, 1)]
}

/// Evaluate a linear combination against a witness.
pub fn evaluate_linear(lin: &Linear, witness: &[Scalar]) -> Scalar {
    lin.iter().map(|&(i, c)| c * witness[i]).sum()
}

/// An evaluation error: either a quadratic gate failed or a post-quadratic
/// assertion was violated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationError {
    /// Gate `index` produced `lhs * rhs != out`.
    GateFailed {
        /// Index of the failing gate in [`R1CS::gates`].
        index: usize,
        /// `Σ aᵢwᵢ` for the failing gate.
        lhs: Scalar,
        /// `Σ bᵢwᵢ` for the failing gate.
        rhs: Scalar,
        /// `Σ cᵢwᵢ` for the failing gate.
        out: Scalar,
    },
    /// A non-negativity assertion failed for witness slot `slot`.
    NonNegativeFailed {
        /// Witness slot whose value went negative.
        slot: usize,
        /// The offending value.
        value: Scalar,
    },
}

impl R1CS {
    /// Check a full witness assignment (including `witness[0] == 1`).
    pub fn evaluate(&self, witness: &[Scalar]) -> Result<(), EvaluationError> {
        assert!(
            witness.len() == self.num_variables,
            "witness has {} values, expected {}",
            witness.len(),
            self.num_variables
        );
        assert_eq!(witness[0], 1, "witness[0] must equal the constant 1");
        for (index, gate) in self.gates.iter().enumerate() {
            let a = evaluate_linear(&gate.a, witness);
            let b = evaluate_linear(&gate.b, witness);
            let c = evaluate_linear(&gate.c, witness);
            if a * b != c {
                return Err(EvaluationError::GateFailed { index, lhs: a, rhs: b, out: c });
            }
        }
        for &R1csAssertion::NonNegative(slot) in &self.assertions {
            if witness[slot] < 0 {
                return Err(EvaluationError::NonNegativeFailed {
                    slot,
                    value: witness[slot],
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConstraintSystemBuilder;

    /// The Phase 2 milestone circuit: `prove_balance_transfer`.
    fn balance_transfer_ir() -> (ConstraintSystem, usize) {
        let mut b = ConstraintSystemBuilder::new("prove_balance_transfer");
        let sender = b.public_input("sender_balance");
        let receiver = b.public_input("receiver_balance");
        let amount = b.secret_input("amount");
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let new_receiver = b.add(receiver, amount);
        let expected = b.add(receiver, amount);
        b.constrain_eq(new_receiver, expected);
        (b.build(), amount)
    }

    /// Bottom-up filler: give every expression slot the value implied by its
    /// children, given assigned public/secret inputs.
    fn satisfy(ir: &ConstraintSystem, witness: &mut [Scalar], r1cs: &R1CS) {
        for (id, e) in ir.exprs.iter().enumerate() {
            let Some(slot) = r1cs.expr_slots[id] else { continue };
            let value = match *e {
                Expr::Const(c) => c,
                Expr::Var(v) => {
                    let is_public = ir
                        .public_inputs
                        .iter()
                        .any(|&p| matches!(ir.exprs[p], Expr::Var(pv) if pv == v));
                    let is_secret = ir
                        .secret_inputs
                        .iter()
                        .any(|&s| matches!(ir.exprs[s], Expr::Var(sv) if sv == v));
                    if is_public {
                        let idx = ir
                            .public_inputs
                            .iter()
                            .position(|&p| matches!(ir.exprs[p], Expr::Var(pv) if pv == v))
                            .unwrap();
                        witness[r1cs.public_slots[idx]]
                    } else if is_secret {
                        let idx = ir
                            .secret_inputs
                            .iter()
                            .position(|&s| matches!(ir.exprs[s], Expr::Var(sv) if sv == v))
                            .unwrap();
                        witness[r1cs.secret_slots[idx]]
                    } else {
                        unreachable!("unclassified variable")
                    }
                }
                Expr::Add(l, r) => witness[r1cs.expr_slots[l].unwrap()] + witness[r1cs.expr_slots[r].unwrap()],
                Expr::Sub(l, r) => witness[r1cs.expr_slots[l].unwrap()] - witness[r1cs.expr_slots[r].unwrap()],
                Expr::Mul(l, r) => witness[r1cs.expr_slots[l].unwrap()] * witness[r1cs.expr_slots[r].unwrap()],
                Expr::Neg(n) => -witness[r1cs.expr_slots[n].unwrap()],
            };
            witness[slot] = value;
        }
    }

    #[test]
    fn lowers_and_satisfies() {
        let (ir, amount_expr) = balance_transfer_ir();
        let r1cs = lower_r1cs(&ir);
        assert_eq!(r1cs.public_slots.len(), 2);
        assert_eq!(r1cs.secret_slots.len(), 1);
        assert_ne!(r1cs.gates.len(), 0);

        let mut witness = vec![0; r1cs.num_variables];
        witness[0] = 1;
        witness[r1cs.public_slots[0]] = 50;
        witness[r1cs.public_slots[1]] = 20;
        witness[r1cs.expr_slots[amount_expr].unwrap()] = 30;
        satisfy(&ir, &mut witness, &r1cs);
        r1cs.evaluate(&witness).unwrap();
    }

    #[test]
    fn lowers_and_rejects_wrong_assignment() {
        let (ir, amount_expr) = balance_transfer_ir();
        let r1cs = lower_r1cs(&ir);
        let mut witness = vec![0; r1cs.num_variables];
        witness[0] = 1;
        witness[r1cs.public_slots[0]] = 50;
        witness[r1cs.public_slots[1]] = 20;
        witness[r1cs.expr_slots[amount_expr].unwrap()] = 100; // violates surplus >= 0
        satisfy(&ir, &mut witness, &r1cs);
        assert!(r1cs.evaluate(&witness).is_err());
    }

    #[test]
    fn constant_is_pinned() {
        let mut b = ConstraintSystemBuilder::new("const");
        let x = b.public_input("x");
        let c = b.constant(7);
        let s = b.add(x, c);
        let ir = b.build();
        let r1cs = lower_r1cs(&ir);

        let mut witness = vec![0; r1cs.num_variables];
        witness[0] = 1;
        witness[r1cs.public_slots[0]] = 5;
        satisfy(&ir, &mut witness, &r1cs); // const node pinned by its gate
        r1cs.evaluate(&witness).unwrap();
        assert_eq!(witness[r1cs.expr_slots[s].unwrap()], 12);
    }

    #[test]
    fn variable_names_carry_over() {
        let (ir, _) = balance_transfer_ir();
        let r1cs = lower_r1cs(&ir);
        let has = |needle: &str| r1cs.names.iter().any(|n| n.as_deref() == Some(needle));
        assert!(has("sender_balance"));
        assert!(has("receiver_balance"));
        assert!(has("amount"));
    }
}