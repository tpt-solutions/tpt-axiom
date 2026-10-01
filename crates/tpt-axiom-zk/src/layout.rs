//! The public-input layout printout: what a verifier actually sees, in order.
//!
//! A `ProofEnvelope` and a halo2 instance column both carry public inputs as a
//! bare positional vector. That is fine for machines and useless for a human
//! deciding whether the statement they are about to accept is the statement
//! they meant. [`InputLayout`] describes that vector slot by slot — name,
//! visibility, declared integer type — and its
//! [`Display`](core::fmt::Display) renders the table, which is what
//! `cargo axiom inspect` prints and what a verifier's log should carry
//! alongside a claim.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr, IntType, Visibility};

/// One slot of the circuit's input vectors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSlot {
    /// Position in `ConstraintSystem::public_inputs` (or `secret_inputs`) — the
    /// order a proving instance column and a `ProofEnvelope::publics` vector
    /// use.
    pub index: usize,
    /// The variable's name, as derived from the Rust parameter.
    pub name: String,
    /// Public or secret; mirrors the IR's own classification.
    pub visibility: Visibility,
    /// The integer type the parameter was declared with, which drives the
    /// backend's range check.
    pub int_type: IntType,
    /// `true` when this public slot is the circuit's *output*: the variable an
    /// `Equal` constraint pins to a computed expression, which is the slot a
    /// verifier reads to learn the result.
    pub is_output: bool,
}

/// The circuit's public and secret input vectors, described slot by slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputLayout {
    /// The circuit's name.
    pub circuit: String,
    /// The public slots, in instance order.
    pub public: Vec<InputSlot>,
    /// The secret slots, in witness order.
    pub secret: Vec<InputSlot>,
}

impl InputLayout {
    /// Describes `ir`'s public and secret input vectors.
    #[must_use]
    pub fn of(ir: &ConstraintSystem) -> Self {
        let output_expr = find_output(ir);
        let describe = |kind: &str, slots: &[usize]| -> Vec<InputSlot> {
            slots
                .iter()
                .enumerate()
                .map(|(index, &expr)| {
                    // A slot that is not a variable reference has no metadata;
                    // `ConstraintSystem::validate` reports that separately, and
                    // the printout names the slot position rather than
                    // inventing a name for it.
                    let var = match ir.exprs.get(expr) {
                        Some(Expr::Var(var)) => Some(*var),
                        _ => None,
                    };
                    let info = var.and_then(|v| ir.variables.get(v));
                    InputSlot {
                        index,
                        name: info.map_or_else(|| format!("{kind}[{index}]"), |i| i.name.clone()),
                        visibility: info.map_or(Visibility::Public, |i| i.visibility),
                        int_type: info.map_or(IntType::I64, |i| i.int_type),
                        is_output: kind == "public" && Some(expr) == output_expr,
                    }
                })
                .collect()
        };
        Self {
            circuit: ir.name.clone(),
            public: describe("public", &ir.public_inputs),
            secret: describe("secret", &ir.secret_inputs),
        }
    }

    /// The public slots only, in instance order — the vector a verifier checks.
    #[must_use]
    pub fn public_slots(&self) -> &[InputSlot] {
        &self.public
    }

    /// The secret slots, in witness order.
    #[must_use]
    pub fn secret_slots(&self) -> &[InputSlot] {
        &self.secret
    }
}

impl fmt::Display for InputLayout {
    /// Renders both vectors as an aligned table:
    ///
    /// ```text
    /// circuit prove_balance_transfer
    ///   public #0  sender_balance    u64
    ///   public #1  receiver_balance  u64
    ///   public #2  return (output)   u64
    ///   secret #0  amount            u64
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "circuit {}", self.circuit)?;
        let rows: Vec<(String, String, &str)> = self
            .public
            .iter()
            .chain(&self.secret)
            .map(|slot| {
                let role = match slot.visibility {
                    Visibility::Public => "public",
                    Visibility::Secret => "secret",
                };
                let kind = if slot.is_output { " (output)" } else { "" };
                (
                    format!("{role} #{}", slot.index),
                    format!("{}{kind}", slot.name),
                    type_name(slot.int_type),
                )
            })
            .collect();
        if rows.is_empty() {
            return writeln!(f, "  (no inputs)");
        }
        let width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
        for (position, name, ty) in rows {
            writeln!(f, "  {position:<10} {name:<width$}  {ty}")?;
        }
        Ok(())
    }
}

/// The public expression an `Equal` constraint pins to a computed node: the
/// circuit's output slot.
///
/// Only constraints with exactly one variable side qualify. A
/// variable-to-variable equality (`assert_eq!(a, b)` on two inputs) is a
/// claim, not an output definition, so it is deliberately not reported as one.
fn find_output(ir: &ConstraintSystem) -> Option<usize> {
    ir.constraints.iter().find_map(|c| match *c {
        Constraint::Equal(l, r) => {
            let l_is_var = is_variable(ir, l);
            let r_is_var = is_variable(ir, r);
            match (l_is_var, r_is_var) {
                (true, false) => Some(l),
                (false, true) => Some(r),
                _ => None,
            }
        }
        _ => None,
    })
}

fn is_variable(ir: &ConstraintSystem, expr: usize) -> bool {
    matches!(ir.exprs.get(expr), Some(Expr::Var(_)))
}

/// The Rust-ish name of an [`IntType`], for the printout.
const fn type_name(int_type: IntType) -> &'static str {
    match (int_type.bits, int_type.signed) {
        (8, false) => "u8",
        (16, false) => "u16",
        (32, false) => "u32",
        (64, false) => "u64",
        (8, true) => "i8",
        (16, true) => "i16",
        (32, true) => "i32",
        _ => "i64",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use tpt_axiom_ir::ConstraintSystemBuilder;

    /// The balance-transfer shape used across the printout tests: two public
    /// inputs, one secret, one public output.
    fn sample_ir() -> ConstraintSystem {
        let mut b = ConstraintSystemBuilder::new("prove_balance_transfer");
        let sender = b.public_input_typed("sender_balance", IntType::U64);
        let receiver = b.public_input_typed("receiver_balance", IntType::U64);
        let amount = b.secret_input_typed("amount", IntType::U64);
        let out = b.output_typed("return", IntType::U64);
        let sum = b.add(receiver, amount);
        b.constrain_eq(out, sum);
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        b.build()
    }

    #[test]
    fn layout_lists_public_slots_in_instance_order() {
        let layout = InputLayout::of(&sample_ir());
        assert_eq!(layout.circuit, "prove_balance_transfer");
        let names: Vec<&str> = layout
            .public_slots()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["sender_balance", "receiver_balance", "return"],
            "public slots follow the IR's declaration order, which is the \
             instance-column order a verifier checks"
        );
        assert_eq!(
            layout
                .public_slots()
                .iter()
                .map(|s| s.index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        // The output slot is marked as such; the others are not.
        assert!(!layout.public_slots()[0].is_output);
        assert!(layout.public_slots()[2].is_output);
        // Secret slots carry the secret visibility and their own types.
        let secret = &layout.secret_slots()[0];
        assert_eq!(secret.name, "amount");
        assert_eq!(secret.visibility, Visibility::Secret);
        assert!(!secret.is_output);
    }

    #[test]
    fn declared_types_reach_the_printout() {
        let mut b = ConstraintSystemBuilder::new("typed");
        b.public_input_typed("small", IntType::U8);
        b.public_input_typed("offset", IntType::I16);
        b.secret_input_typed("delta", IntType::U32);
        let layout = InputLayout::of(&b.build());
        let types: Vec<&str> = layout
            .public_slots()
            .iter()
            .chain(layout.secret_slots())
            .map(|s| type_name(s.int_type))
            .collect();
        assert_eq!(types, ["u8", "i16", "u32"]);
    }

    #[test]
    fn the_printout_names_every_slot_and_marks_the_output() {
        let printed = InputLayout::of(&sample_ir()).to_string();
        assert!(printed.contains("circuit prove_balance_transfer"));
        assert!(printed.contains("public #0  sender_balance"));
        assert!(printed.contains("receiver_balance"));
        assert!(printed.contains("return (output)"));
        assert!(printed.contains("secret #0  amount"));
        assert!(printed.contains("u64"));
    }

    #[test]
    fn a_circuit_with_no_inputs_says_so() {
        let mut b = ConstraintSystemBuilder::new("empty");
        let one = b.constant(1);
        b.constrain_non_negative(one);
        let printed = InputLayout::of(&b.build()).to_string();
        assert!(printed.contains("(no inputs)"), "printed: {printed}");
    }

    #[test]
    fn a_circuit_without_an_output_marks_no_slot() {
        let mut b = ConstraintSystemBuilder::new("no_output");
        let x = b.public_input_typed("x", IntType::I64);
        let y = b.secret_input_typed("y", IntType::I64);
        let d = b.sub(x, y);
        b.constrain_non_negative(d);
        let layout = InputLayout::of(&b.build());
        assert!(
            layout.public_slots().iter().all(|s| !s.is_output),
            "a circuit whose only constraint is a subtraction has no output slot"
        );
    }

    #[test]
    fn a_malformed_slot_is_printed_by_position_not_panicked() {
        // An input slot that is not a variable reference has no metadata; the
        // printout names it by position instead of inventing a name.
        let mut ir = sample_ir();
        let computed = ir.exprs.len();
        ir.exprs.push(Expr::Const(7));
        ir.var_exprs.push(computed);
        ir.public_inputs.push(computed);
        let layout = InputLayout::of(&ir);
        assert_eq!(layout.public_slots()[3].name, "public[3]");
        assert!(layout.to_string().contains("public[3]"));
    }
}
