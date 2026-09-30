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

    /// The exact `[min, max]` of this type as `i128` bounds.
    ///
    /// Unlike [`Self::max_value`] this is lossless for every representable
    /// type, including signed `i64`.
    #[must_use]
    pub const fn bounds(self) -> (i128, i128) {
        if self.signed {
            let max = if self.bits >= 127 {
                i128::MAX
            } else {
                (1i128 << (self.bits - 1)) - 1
            };
            let min = if self.bits >= 127 {
                i128::MIN
            } else {
                -(1i128 << (self.bits - 1))
            };
            (min, max)
        } else if self.bits >= 127 {
            (0, i128::MAX)
        } else {
            (0, (1i128 << self.bits) - 1)
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

/// The conservative bit capacity below which field arithmetic is a faithful
/// image of integer arithmetic.
///
/// Every real backend field (Pallas, BLS12-381 scalar) exceeds 254 bits, so
/// any intermediate statically bounded at or above this many bits could wrap
/// the field modulus and satisfy a constraint that integer arithmetic would
/// not; [`ConstraintSystem::validate`] rejects such circuits up front.
pub const MAX_FAITHFUL_BITS: u32 = 250;

/// A complete, backend-agnostic arithmetic program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstraintSystem {
    /// Circuit name (derived from the annotated function name).
    pub name: String,
    /// Metadata for every named variable; index is the `Var` id.
    pub variables: Vec<VariableInfo>,
    /// Expression table; `Expr::Var(i)` indexes into `variables`.
    pub exprs: Vec<Expr>,
    /// Expression id of each named variable (index = variable id).
    ///
    /// Precomputed by the builder so lookups are O(1) instead of a scan over
    /// `exprs`; [`ConstraintSystem::validate`] re-checks the invariant.
    pub var_exprs: Vec<ExprId>,
    /// `ExprId`s declared as public inputs (including public outputs, which
    /// are public inputs in verifier terms).
    pub public_inputs: Vec<ExprId>,
    /// `ExprId`s declared as secret witnesses.
    pub secret_inputs: Vec<ExprId>,
    /// The logical constraints.
    pub constraints: Vec<Constraint>,
}

/// Ways a [`ConstraintSystem`] can be malformed.
///
/// Systems built exclusively through
/// [`ConstraintSystemBuilder`] satisfy every
/// structural rule by construction; the bit-width rule is the one a builder
/// can legitimately violate (deep `*` chains), which is why backends are
/// expected to call [`ConstraintSystem::validate`] before consuming an IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CircuitError {
    /// `exprs[expr]` references variable `var`, which has no metadata.
    UnknownVariable {
        /// The offending expression node.
        expr: ExprId,
        /// The out-of-range variable id.
        var: usize,
    },
    /// `var_exprs[var]` does not point back at `exprs[expr]`.
    VarExprMismatch {
        /// The variable whose map entry is wrong.
        var: usize,
    },
    /// Expression `expr` references node `operand`, which does not exist.
    OutOfRangeOperand {
        /// The offending expression node.
        expr: ExprId,
        /// The out-of-range operand slot.
        operand: ExprId,
    },
    /// Expression `expr` references node `operand` that comes *after* it;
    /// the IR is a DAG over earlier nodes only.
    ForwardReference {
        /// The offending expression node.
        expr: ExprId,
        /// The later node it references.
        operand: ExprId,
    },
    /// Public or secret input `slot` points at a node that is not a variable
    /// reference (or points past the table entirely).
    InputNotVariable {
        /// Index into `public_inputs`/`secret_inputs`.
        slot: usize,
        /// Which input list: `"public"` or `"secret"`.
        kind: &'static str,
    },
    /// Input slot's variable is declared with the other visibility (e.g. a
    /// public slot pointing at a secret variable).
    InputVisibilityMismatch {
        /// Index into `public_inputs`/`secret_inputs`.
        slot: usize,
        /// Which input list the slot lives in.
        kind: &'static str,
    },
    /// Constraint `index` references a node that does not exist.
    ConstraintOutOfRange {
        /// Index into `constraints`.
        index: usize,
    },
    /// The static worst-case bit width of `expr` reaches
    /// [`MAX_FAITHFUL_BITS`]: a proving field could wrap, so the circuit's
    /// integer semantics are no longer faithful over the field.
    IntermediateOverflow {
        /// The expression whose worst-case width is too large.
        expr: ExprId,
        /// Its worst-case bit width.
        bits: u32,
    },
}

impl core::fmt::Display for CircuitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::UnknownVariable { expr, var } => {
                write!(f, "expression #{expr} references unknown variable {var}")
            }
            Self::VarExprMismatch { var } => {
                write!(f, "variable {var}'s expression map entry is inconsistent")
            }
            Self::OutOfRangeOperand { expr, operand } => {
                write!(f, "expression #{expr} references out-of-range node {operand}")
            }
            Self::ForwardReference { expr, operand } => write!(
                f,
                "expression #{expr} references later node {operand}; operands must be earlier nodes"
            ),
            Self::InputNotVariable { slot, kind } => {
                write!(f, "{kind} input slot {slot} is not a variable expression")
            }
            Self::InputVisibilityMismatch { slot, kind } => write!(
                f,
                "{kind} input slot {slot} points at a variable declared with the other visibility"
            ),
            Self::ConstraintOutOfRange { index } => {
                write!(f, "constraint #{index} references an out-of-range node")
            }
            Self::IntermediateOverflow { expr, bits } => write!(
                f,
                "expression #{expr} can reach {bits} bits; intermediates must stay below \
                 {MAX_FAITHFUL_BITS} bits or the field may wrap and break integer semantics"
            ),
        }
    }
}

impl core::error::Error for CircuitError {}

/// The static worst-case magnitude, in bits, of every expression node.
///
/// Per node: constants contribute their magnitude, variables their declared
/// width, and operations the usual interval-arithmetic growth (`+`/`-` add a
/// carry bit, `*` adds widths). Saturating at `u32::MAX`; callers compare
/// against [`MAX_FAITHFUL_BITS`].
#[must_use]
pub fn bit_bounds(ir: &ConstraintSystem) -> Vec<u32> {
    let mut bounds: Vec<u32> = Vec::with_capacity(ir.exprs.len());
    for expr in &ir.exprs {
        let bound = match *expr {
            Expr::Const(v) => const_magnitude_bits(v),
            Expr::Var(var) => ir
                .variables
                .get(var)
                .map_or(64, |info| info.int_type.bits),
            Expr::Add(l, r) | Expr::Sub(l, r) => add_bounds(&bounds, l, r),
            Expr::Mul(l, r) => mul_bounds(&bounds, l, r),
            Expr::Neg(n) => bounds.get(n).copied().unwrap_or(0),
        };
        bounds.push(bound);
    }
    bounds
}

/// Bits needed for the magnitude of a constant (`i64::MIN` has magnitude
/// `2^63`).
const fn const_magnitude_bits(v: i64) -> u32 {
    if v == 0 {
        0
    } else {
        64 - v.unsigned_abs().leading_zeros()
    }
}

/// `|l ± r| ≤ |l| + |r|`, i.e. one carry bit over the wider operand.
fn add_bounds(bounds: &[u32], l: ExprId, r: ExprId) -> u32 {
    let l = bounds.get(l).copied().unwrap_or(0);
    let r = bounds.get(r).copied().unwrap_or(0);
    l.max(r).saturating_add(1)
}

fn mul_bounds(bounds: &[u32], l: ExprId, r: ExprId) -> u32 {
    let l = bounds.get(l).copied().unwrap_or(0);
    let r = bounds.get(r).copied().unwrap_or(0);
    l.saturating_add(r)
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

    /// The expression node of variable `var`, via the precomputed map.
    ///
    /// Returns `None` when the map does not cover `var` (an IR not built by
    /// the builder) or the entry is inconsistent; use
    /// [`Self::validate`] to diagnose that.
    #[must_use]
    pub fn var_expr_id(&self, var: usize) -> Option<ExprId> {
        let id = *self.var_exprs.get(var)?;
        match self.exprs.get(id) {
            Some(Expr::Var(v)) if *v == var => Some(id),
            _ => None,
        }
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

    /// Checks the IR is well-formed: every reference in range and pointing at
    /// earlier nodes, every input slot a matching-visibility variable, the
    /// variable→expression map consistent, and every intermediate's static
    /// worst-case bit width strictly below [`MAX_FAITHFUL_BITS`] (past that a
    /// proving field could wrap and the integer semantics stop being
    /// faithful).
    ///
    /// Backends call this before compiling; a malformed IR otherwise surfaces
    /// as a panic deep inside a lowering pass.
    ///
    /// # Errors
    /// [`CircuitError`] naming the first structural or bit-width violation.
    pub fn validate(&self) -> Result<(), CircuitError> {
        for (id, expr) in self.exprs.iter().enumerate() {
            match *expr {
                Expr::Const(_) => {}
                Expr::Var(var) => {
                    if var >= self.variables.len() {
                        return Err(CircuitError::UnknownVariable { expr: id, var });
                    }
                    if self.var_exprs.get(var) != Some(&id) {
                        return Err(CircuitError::VarExprMismatch { var });
                    }
                }
                Expr::Add(l, r) | Expr::Sub(l, r) | Expr::Mul(l, r) => {
                    for operand in [l, r] {
                        if operand >= self.exprs.len() {
                            return Err(CircuitError::OutOfRangeOperand {
                                expr: id,
                                operand,
                            });
                        }
                        if operand >= id {
                            return Err(CircuitError::ForwardReference {
                                expr: id,
                                operand,
                            });
                        }
                    }
                }
                Expr::Neg(n) => {
                    if n >= self.exprs.len() {
                        return Err(CircuitError::OutOfRangeOperand { expr: id, operand: n });
                    }
                    if n >= id {
                        return Err(CircuitError::ForwardReference { expr: id, operand: n });
                    }
                }
            }
        }
        for (kind, slots) in [("public", &self.public_inputs), ("secret", &self.secret_inputs)] {
            for (slot, &id) in slots.iter().enumerate() {
                let Some(Expr::Var(var)) = self.exprs.get(id) else {
                    return Err(CircuitError::InputNotVariable { slot, kind });
                };
                let Some(info) = self.variables.get(*var) else {
                    return Err(CircuitError::UnknownVariable { expr: id, var: *var });
                };
                let expected = match kind {
                    "public" => Visibility::Public,
                    _ => Visibility::Secret,
                };
                if info.visibility != expected {
                    return Err(CircuitError::InputVisibilityMismatch { slot, kind });
                }
            }
        }
        for (index, constraint) in self.constraints.iter().enumerate() {
            let out_of_range = match *constraint {
                Constraint::Zero(e) | Constraint::NonNegative(e) => e >= self.exprs.len(),
                Constraint::Equal(l, r) => l >= self.exprs.len() || r >= self.exprs.len(),
            };
            if out_of_range {
                return Err(CircuitError::ConstraintOutOfRange { index });
            }
        }
        for (expr, bits) in bit_bounds(self).into_iter().enumerate() {
            if bits >= MAX_FAITHFUL_BITS {
                return Err(CircuitError::IntermediateOverflow { expr, bits });
            }
        }
        Ok(())
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
        self.system.var_exprs.push(self.system.exprs.len() - 1);
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

    #[test]
    fn int_type_bounds_are_exact() {
        assert_eq!(IntType::U8.bounds(), (0, 255));
        assert_eq!(IntType::I8.bounds(), (-128, 127));
        assert_eq!(IntType::I64.bounds(), (i128::from(i64::MIN), i128::from(i64::MAX)));
        assert_eq!(IntType::U64.bounds(), (0, i128::from(u64::MAX)));
    }

    #[test]
    fn builder_built_systems_validate() {
        let ir = balance_transfer_builder().build();
        assert!(ir.validate().is_ok(), "builder output must be well-formed");
        // The precomputed variable map gives O(1) lookups.
        assert_eq!(ir.var_expr_id(2), ir.variable_id("amount"));
    }

    fn balance_transfer_builder() -> ConstraintSystemBuilder {
        let mut b = ConstraintSystemBuilder::new("validate_me");
        let sender = b.public_input("sender");
        let receiver = b.public_input("receiver");
        let amount = b.secret_input("amount");
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let new_receiver = b.add(receiver, amount);
        let expected = b.add(receiver, amount);
        b.constrain_eq(new_receiver, expected);
        b
    }

    #[test]
    fn forward_reference_is_rejected() {
        let mut ir = ConstraintSystem {
            name: String::from("bad"),
            ..Default::default()
        };
        // Node 0 references node 1 (which does not exist yet).
        ir.exprs.push(Expr::Add(1, 1));
        let err = ir.validate().expect_err("forward reference must fail");
        assert_eq!(
            err,
            CircuitError::OutOfRangeOperand {
                expr: 0,
                operand: 1
            }
        );
        ir.exprs.push(Expr::Var(0));
        // Now node 1 exists but is *later* than its operand slot 1 → forward.
        let err = ir.validate().expect_err("self reference is a forward reference");
        assert!(matches!(err, CircuitError::ForwardReference { .. }), "{err}");
    }

    #[test]
    fn inconsistent_var_map_is_rejected() {
        let mut ir = balance_transfer_builder().build();
        ir.var_exprs[0] = 99;
        let err = ir.validate().expect_err("mangled map must fail");
        assert_eq!(err, CircuitError::VarExprMismatch { var: 0 });
    }

    #[test]
    fn input_slot_pointing_at_non_variable_is_rejected() {
        let mut ir = balance_transfer_builder().build();
        ir.public_inputs[0] = ir.variable_id("amount").expect("amount is an expr");
        let err = ir.validate().expect_err("public slot at a secret variable");
        assert!(matches!(
            err,
            CircuitError::InputVisibilityMismatch { kind: "public", .. }
        ));
    }

    #[test]
    fn deep_mul_chain_is_rejected_for_field_wraparound() {
        // (x * y) * w * z * v with 64-bit inputs: the static bound reaches
        // 5*64 = 320 bits, far past any proving field — a field image could
        // wrap and satisfy constraints integer arithmetic would not.
        let mut b = ConstraintSystemBuilder::new("deep_mul");
        let x = b.public_input("x");
        let y = b.secret_input("y");
        let w = b.secret_input("w");
        let z = b.secret_input("z");
        let v = b.secret_input("v");
        let m1 = b.mul(x, y);
        let m2 = b.mul(m1, w);
        let m3 = b.mul(m2, z);
        let m4 = b.mul(m3, v);
        b.constrain_zero(m4);
        let ir = b.build();
        let err = ir.validate().expect_err("320-bit intermediate must fail");
        let CircuitError::IntermediateOverflow { expr, bits } = err else {
            panic!("unexpected error: {err}");
        };
        // m3 is the first node to cross the line: 4 inputs wide = 256 bits.
        assert_eq!(expr, m3);
        assert_eq!(bits, 256);
    }

    #[test]
    fn moderate_arithmetic_still_validates() {
        // The macro's typical shape: several i64 additions/muls on top of
        // 64-bit inputs. Their worst case (~130 bits) is well under the
        // ~254-bit field capacity, so the circuit must validate.
        let mut b = ConstraintSystemBuilder::new("moderate");
        let a = b.public_input("a");
        let b2 = b.public_input("b");
        let c = b.public_input("c");
        let d = b.secret_input("d");
        let ab = b.mul(a, b2);
        let cd = b.mul(c, d);
        let sum = b.add(ab, cd);
        b.constrain_non_negative(sum);
        assert!(b.build().validate().is_ok());
    }
}
