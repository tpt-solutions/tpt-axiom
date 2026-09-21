//! Core IR types: the expression DAG, variable classification, constraints,
//! and the builder used by `#[zk_provable]`-generated code.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::Scalar;

/// A handle to a node in the constraint system's expression table.
pub type ExprId = usize;

/// Whether a variable is public (visible to the verifier) or secret (witness).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// Public input or output of the circuit.
    Public,
    /// Secret witness value, hidden from the verifier.
    Secret,
}

/// Metadata about a named variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableInfo {
    /// Human-readable name carried over from the Rust function's parameter.
    pub name: String,
    /// Public or secret.
    pub visibility: Visibility,
}

/// A node in the arithmetic expression DAG.
///
/// Nodes are stored in a flat table; `ExprId`s refer into that table. Leaves
/// are either constants or named input/witness variables; internal nodes are
/// combinations of earlier nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A constant.
    Const(Scalar),
    /// Reference to the variable at `variables[index]`.
    Var(usize),
    /// `l + r`
    Add(ExprId, ExprId),
    /// `l - r`
    Sub(ExprId, ExprId),
    /// `l * r`
    Mul(ExprId, ExprId),
    /// `-e`
    Neg(ExprId),
}

/// A logical constraint the circuit must enforce.
///
/// Backends lower these into field gates; `NonNegative` additionally requires
/// bit-decomposition (range checks) by the backend, which is exactly how
/// comparisons like `a >= b` become proveable in practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// Expression 0: `e == 0`.
    Zero(ExprId),
    /// `l == r`.
    Equal(ExprId, ExprId),
    /// `e >= 0` (public-keyable only via range-check decomposition).
    NonNegative(ExprId),
}

/// A complete, backend-agnostic arithmetic program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstraintSystem {
    /// Circuit name (derived from the annotated function name).
    pub name: String,
    /// Metadata for every named variable; index is the `Var` id.
    pub variables: Vec<VariableInfo>,
    /// Expression table; `Expr::Var(i)` indexes into `variables`.
    pub exprs: Vec<Expr>,
    /// `ExprId`s declared as public inputs (including public outputs, which
    /// are public inputs in verifier terms).
    pub public_inputs: Vec<ExprId>,
    /// `ExprId`s declared as secret witnesses.
    pub secret_inputs: Vec<ExprId>,
    /// The logical constraints.
    pub constraints: Vec<Constraint>,
}

impl ConstraintSystem {
    /// Returns the named variable's id, if it is a variable expression.
    pub fn variable_id(&self, name: &str) -> Option<usize> {
        self.exprs.iter().enumerate().find_map(|(id, e)| match e {
            Expr::Var(v) if self.variables.get(*v).map(|i| i.name.as_str()) == Some(name) => Some(id),
            _ => None,
        })
    }

    /// Number of named public inputs + outputs.
    pub fn num_public(&self) -> usize {
        self.public_inputs.len()
    }

    /// Number of secret witness variables declared.
    pub fn num_secret(&self) -> usize {
        self.secret_inputs.len()
    }

    /// Render the circuit as a multi-line textual description (diagnostics and
    /// Phase 3 equivalence checking).
    pub fn describe(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("circuit {} {{\n", self.name));
        for (kind, ids) in [("public", &self.public_inputs), ("secret", &self.secret_inputs)] {
            for &id in ids {
                if let Expr::Var(v) = &self.exprs[id] {
                    if let Some(info) = self.variables.get(*v) {
                        out.push_str(&format!("  {kind} input {}\n", info.name));
                    }
                }
            }
        }
        out.push_str("  constraints\n");
        for c in &self.constraints {
            out.push_str(&format!("    {c:?}\n"));
        }
        out.push('}');
        out
    }
}

/// Incremental builder for [`ConstraintSystem`]; used by the code that the
/// `#[zk_provable]` macro generates.
#[derive(Debug, Default)]
pub struct ConstraintSystemBuilder {
    system: ConstraintSystem,
    named_exprs: Vec<Option<String>>,
}

impl ConstraintSystemBuilder {
    /// Begin building a circuit with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            system: ConstraintSystem {
                name: name.into(),
                ..Default::default()
            },
            named_exprs: Vec::new(),
        }
    }

    /// Declare a public input variable and return its expression id.
    pub fn public_input(&mut self, name: &str) -> ExprId {
        self.declare_var(name, Visibility::Public)
    }

    /// Declare a public output variable (a public input to the verifier).
    pub fn output(&mut self, name: &str) -> ExprId {
        self.declare_var(name, Visibility::Public)
    }

    /// Declare a secret witness variable and return its expression id.
    pub fn secret_input(&mut self, name: &str) -> ExprId {
        self.declare_var(name, Visibility::Secret)
    }

    fn declare_var(&mut self, name: &str, visibility: Visibility) -> ExprId {
        let var_id = self.system.variables.len();
        self.system.variables.push(VariableInfo {
            name: name.to_owned(),
            visibility,
        });
        self.system.exprs.push(Expr::Var(var_id));
        self.named_exprs.push(Some(name.to_owned()));
        let id = self.system.exprs.len() - 1;
        match visibility {
            Visibility::Public => self.system.public_inputs.push(id),
            Visibility::Secret => self.system.secret_inputs.push(id),
        }
        id
    }

    /// A constant expression.
    pub fn constant(&mut self, value: Scalar) -> ExprId {
        self.push(Expr::Const(value))
    }

    /// `l + r`
    pub fn add(&mut self, l: ExprId, r: ExprId) -> ExprId {
        self.push(Expr::Add(l, r))
    }

    /// `l - r`
    pub fn sub(&mut self, l: ExprId, r: ExprId) -> ExprId {
        self.push(Expr::Sub(l, r))
    }

    /// `l * r`
    pub fn mul(&mut self, l: ExprId, r: ExprId) -> ExprId {
        self.push(Expr::Mul(l, r))
    }

    /// `-e`
    pub fn neg(&mut self, e: ExprId) -> ExprId {
        self.push(Expr::Neg(e))
    }

    /// Enforce `e == 0`.
    pub fn constrain_zero(&mut self, e: ExprId) {
        self.system.constraints.push(Constraint::Zero(e));
    }

    /// Enforce `l == r`.
    pub fn constrain_eq(&mut self, l: ExprId, r: ExprId) {
        self.system.constraints.push(Constraint::Equal(l, r));
    }

    /// Enforce `e >= 0`.
    pub fn constrain_non_negative(&mut self, e: ExprId) {
        self.system.constraints.push(Constraint::NonNegative(e));
    }

    fn push(&mut self, e: Expr) -> ExprId {
        self.named_exprs.push(None);
        self.system.exprs.push(e);
        self.system.exprs.len() - 1
    }

    /// Finalize into a [`ConstraintSystem`].
    pub fn build(self) -> ConstraintSystem {
        self.system
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_balance_transfer_ir() {
        let mut b = ConstraintSystemBuilder::new("prove_balance_transfer");
        let sender = b.public_input("sender_balance");
        let receiver = b.public_input("receiver_balance");
        let amount = b.secret_input("amount");
        // assert!(sender_balance >= amount)  =>  sender - amount >= 0
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        // let new_receiver_balance = receiver + amount
        let new_receiver = b.add(receiver, amount);
        // assert_eq!(new_receiver_balance, receiver + amount)
        let rhs = b.add(receiver, amount);
        b.constrain_eq(new_receiver, rhs);

        let ir = b.build();
        assert_eq!(ir.name, "prove_balance_transfer");
        assert_eq!(ir.num_public(), 2);
        assert_eq!(ir.num_secret(), 1);
        assert_eq!(ir.constraints.len(), 2);
        assert_eq!(
            ir.constraints[0],
            Constraint::NonNegative(surplus)
        );
        assert_eq!(ir.constraints[1], Constraint::Equal(new_receiver, rhs));
        assert_eq!(ir.variable_id("amount"), Some(amount));
    }

    #[test]
    fn describe_renders() {
        let mut b = ConstraintSystemBuilder::new("f");
        let x = b.public_input("x");
        b.constrain_zero(x);
        let text = b.build().describe();
        assert!(text.contains("circuit f"));
        assert!(text.contains("public input x"));
    }
}