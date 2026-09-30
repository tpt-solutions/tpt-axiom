//! Core IR types: the expression DAG, variable classification, constraints,
//! and the builder used by `#[zk_provable]`-generated code.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

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

/// The declared integer type of a variable: a bit width plus signedness.
///
/// The macro records the parameter's Rust type here so backends can range
/// check each variable against *its own* width instead of a single circuit-wide
/// signed-`i64` assumption. A `u8` parameter is proven to lie in `[0, 2^8)`; an
/// `i64` parameter is proven to lie in `[-2^63, 2^63)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntType {
    /// Width of the type in bits (`8`, `16`, `32`, `64`).
    pub bits: u32,
    /// Whether the value may be negative.
    pub signed: bool,
}

impl IntType {
    /// Signed 8-bit.
    pub const I8: Self = Self::new(8, true);
    /// Signed 16-bit.
    pub const I16: Self = Self::new(16, true);
    /// Signed 32-bit.
    pub const I32: Self = Self::new(32, true);
    /// Signed 64-bit, the default assumed for hand-built IR.
    pub const I64: Self = Self::new(64, true);
    /// Unsigned 8-bit.
    pub const U8: Self = Self::new(8, false);
    /// Unsigned 16-bit.
    pub const U16: Self = Self::new(16, false);
    /// Unsigned 32-bit.
    pub const U32: Self = Self::new(32, false);
    /// Unsigned 64-bit.
    pub const U64: Self = Self::new(64, false);

    /// Builds a type description from a bit width and signedness.
    #[must_use]
    pub const fn new(bits: u32, signed: bool) -> Self {
        Self { bits, signed }
    }

    /// Parses a Rust integer primitive name (`u8`, `i32`, ...).
    ///
    /// Returns `None` for any non-integer type name.
    #[must_use]
    pub fn from_type_name(name: &str) -> Option<Self> {
        let (signed, bits) = match name {
            "u8" => (false, 8),
            "u16" => (false, 16),
            "u32" => (false, 32),
            "u64" | "usize" => (false, 64),
            "i8" => (true, 8),
            "i16" => (true, 16),
            "i32" => (true, 32),
            "i64" | "isize" => (true, 64),
            _ => return None,
        };
        Some(Self { bits, signed })
    }

    /// The largest value this type can represent (`u64`); saturating for
    /// widths above 64 bits.
    #[must_use]
    pub const fn max_value(self) -> u64 {
        if self.bits >= 64 {
            u64::MAX
        } else {
            (1u64 << self.bits) - 1
        }
    }

    /// The offset that maps a signed value into `[0, 2^bits)`.
    #[must_use]
    pub const fn signed_offset(self) -> u64 {
        if self.signed && self.bits > 0 {
            1u64 << (self.bits - 1)
        } else {
            0
        }
    }
}

impl Default for IntType {
    fn default() -> Self {
        Self::I64
    }
}

/// Metadata about a named variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableInfo {
    /// Human-readable name carried over from the Rust function's parameter.
    pub name: String,
    /// Public or secret.
    pub visibility: Visibility,
    /// The integer type the parameter was declared with; drives range checks.
    pub int_type: IntType,
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
    #[must_use]
    pub fn variable_id(&self, name: &str) -> Option<usize> {
        self.exprs.iter().enumerate().find_map(|(id, e)| match e {
            Expr::Var(v) if self.variables.get(*v).map(|i| i.name.as_str()) == Some(name) => {
                Some(id)
            }
            _ => None,
        })
    }

    /// The declared integer type of a variable id, defaulting to signed
    /// `i64` when the id is out of range.
    #[must_use]
    pub fn int_type(&self, variable: usize) -> IntType {
        self.variables
            .get(variable)
            .map_or(IntType::I64, |info| info.int_type)
    }

    /// The declared integer type of the variable an expression reads, if it is
    /// a variable reference.
    #[must_use]
    pub fn expr_int_type(&self, id: ExprId) -> Option<IntType> {
        match self.exprs.get(id) {
            Some(Expr::Var(v)) => Some(self.int_type(*v)),
            _ => None,
        }
    }

    /// Number of named public inputs + outputs.
    #[must_use]
    pub fn num_public(&self) -> usize {
        self.public_inputs.len()
    }

    /// Number of secret witness variables declared.
    #[must_use]
    pub fn num_secret(&self) -> usize {
        self.secret_inputs.len()
    }

    /// Render the circuit as a multi-line textual description (diagnostics and
    /// Phase 3 equivalence checking).
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let _ = core::writeln!(&mut out, "circuit {} {{", self.name);
        for (kind, ids) in [
            ("public", &self.public_inputs),
            ("secret", &self.secret_inputs),
        ] {
            for &id in ids {
                if let Expr::Var(v) = &self.exprs[id] {
                    if let Some(info) = self.variables.get(*v) {
                        let _ = core::writeln!(
                            &mut out,
                            "  {kind} input {} : {}{}",
                            info.name,
                            if info.int_type.signed { "i" } else { "u" },
                            info.int_type.bits
                        );
                    }
                }
            }
        }
        out.push_str("  constraints\n");
        for c in &self.constraints {
            let _ = core::writeln!(&mut out, "    {c:?}");
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
    ///
    /// Assumes signed `i64` semantics; use [`Self::public_input_typed`] to
    /// carry a narrower or unsigned declared type.
    pub fn public_input(&mut self, name: &str) -> ExprId {
        self.public_input_typed(name, IntType::I64)
    }

    /// Declare a public input variable with an explicit integer type.
    pub fn public_input_typed(&mut self, name: &str, int_type: IntType) -> ExprId {
        self.declare_var(name, Visibility::Public, int_type)
    }

    /// Declare a public output variable (a public input to the verifier).
    ///
    /// Assumes signed `i64` semantics; use [`Self::output_typed`] otherwise.
    pub fn output(&mut self, name: &str) -> ExprId {
        self.output_typed(name, IntType::I64)
    }

    /// Declare a public output variable with an explicit integer type.
    pub fn output_typed(&mut self, name: &str, int_type: IntType) -> ExprId {
        self.declare_var(name, Visibility::Public, int_type)
    }

    /// Declare a secret witness variable and return its expression id.
    ///
    /// Assumes signed `i64` semantics; use [`Self::secret_input_typed`] to
    /// carry a narrower or unsigned declared type.
    pub fn secret_input(&mut self, name: &str) -> ExprId {
        self.secret_input_typed(name, IntType::I64)
    }

    /// Declare a secret witness variable with an explicit integer type.
    pub fn secret_input_typed(&mut self, name: &str, int_type: IntType) -> ExprId {
        self.declare_var(name, Visibility::Secret, int_type)
    }

    fn declare_var(&mut self, name: &str, visibility: Visibility, int_type: IntType) -> ExprId {
        let var_id = self.system.variables.len();
        self.system.variables.push(VariableInfo {
            name: name.to_owned(),
            visibility,
            int_type,
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
    #[must_use]
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
        assert_eq!(ir.constraints[0], Constraint::NonNegative(surplus));
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
        assert!(text.contains("public input x : i64"));
    }

    #[test]
    fn typed_declarations_carry_their_width() {
        let mut b = ConstraintSystemBuilder::new("f");
        let a = b.public_input_typed("a", IntType::U8);
        let c = b.secret_input_typed("c", IntType::I16);
        b.output_typed("return", IntType::U32);
        let ir = b.build();

        assert_eq!(ir.expr_int_type(a), Some(IntType::U8));
        assert_eq!(ir.expr_int_type(c), Some(IntType::I16));
        assert_eq!(IntType::U8.max_value(), 255);
        assert_eq!(IntType::I8.signed_offset(), 128);
        assert_eq!(IntType::U8.signed_offset(), 0);
        assert!(ir.describe().contains("public input a : u8"));
        assert!(ir.describe().contains("secret input c : i16"));
    }

    #[test]
    fn untyped_declarations_default_to_signed_i64() {
        let mut b = ConstraintSystemBuilder::new("f");
        let a = b.public_input("a");
        assert_eq!(b.build().expr_int_type(a), Some(IntType::I64));
    }

    #[test]
    fn int_type_parses_rust_primitives() {
        assert_eq!(IntType::from_type_name("u8"), Some(IntType::U8));
        assert_eq!(IntType::from_type_name("isize"), Some(IntType::I64));
        assert_eq!(IntType::from_type_name("f64"), None);
        assert_eq!(IntType::from_type_name("String"), None);
    }
}
