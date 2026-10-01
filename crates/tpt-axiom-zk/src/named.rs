//! Name-keyed witness assignment: build a witness by variable *name* instead
//! of by positional slot.
//!
//! [`witness::check`] takes two positional slices whose order is a property of
//! the circuit's declaration order. That is exactly what a backend wants, but
//! it is a poor interface for a caller (a JSON input file, a CLI `--in`, an
//! audit bundle): silently transposing two adjacent `u64` fields produces a
//! well-formed witness for the wrong statement.
//!
//! [`NamedWitness`] closes that hole. Values are keyed by the names
//! `#[zk_provable]` derived from the Rust parameters, and
//! [`NamedWitness::resolve`] turns the map into the positional slices only
//! after checking it against the IR: every public and secret input must be
//! present exactly once, with nothing extra. Resolution is a *checked*
//! projection, not a guess.

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tpt_axiom_ir::{ConstraintSystem, Expr, Scalar};

/// A witness keyed by input name, resolved against a circuit's IR.
///
/// Build with [`NamedWitness::new`] and [`NamedWitness::set`], or with the
/// generated `Inputs` struct's `named()` method; then call
/// [`NamedWitness::resolve`] to get the positional [`WitnessValues`].
///
/// ```
/// use tpt_axiom_zk::tpt_axiom_ir::ConstraintSystemBuilder;
/// use tpt_axiom_zk::NamedWitness;
///
/// let mut b = ConstraintSystemBuilder::new("transfer");
/// let sender = b.public_input("sender");
/// let amount = b.secret_input("amount");
/// let surplus = b.sub(sender, amount);
/// b.constrain_non_negative(surplus);
/// let ir = b.build();
///
/// let witness = NamedWitness::new()
///     .with("sender", 50)
///     .with("amount", 30);
/// let values = witness.resolve(&ir).expect("complete witness");
/// assert_eq!(values.public(), &[50]);
/// assert_eq!(values.secret(), &[30]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamedWitness {
    /// Insertion-ordered `(name, value)` pairs. A `Vec` rather than a map keeps
    /// this crate `no_std` without a hash container and preserves write order,
    /// which makes resolution errors deterministic.
    entries: Vec<(String, Scalar)>,
}

impl NamedWitness {
    /// An empty witness.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Sets (or replaces) the value for `name`, returning the witness.
    ///
    /// Replacing silently is deliberate: it lets a caller layer defaults and
    /// then override specific fields. A genuinely ambiguous input — two
    /// different fields bound to one name — cannot be expressed here, because
    /// names come from the circuit rather than the caller.
    #[must_use]
    pub fn with(mut self, name: &str, value: Scalar) -> Self {
        self.set(name, value);
        self
    }

    /// Sets (or replaces) the value for `name`.
    pub fn set(&mut self, name: &str, value: Scalar) {
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((name.to_owned(), value)),
        }
    }

    /// The value recorded for `name`, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Scalar> {
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
    }

    /// How many named values are recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no named values are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Checks this witness against `ir` and projects it onto positional
    /// slices in the IR's declaration order.
    ///
    /// # Errors
    ///
    /// * [`NamedWitnessError::UnknownInput`] — a name this circuit does not
    ///   declare, which is almost always a typo or a field belonging to a
    ///   different circuit's input struct.
    /// * [`NamedWitnessError::MissingInput`] — a declared public or secret
    ///   input with no value. Refusing here, rather than filling in a zero, is
    ///   what stops a half-specified statement from being proved as though the
    ///   absent field were zero.
    /// * [`NamedWitnessError::SlotNotAVariable`] — the IR itself is malformed
    ///   (an input slot that is not a variable reference).
    pub fn resolve(&self, ir: &ConstraintSystem) -> Result<WitnessValues, NamedWitnessError> {
        let mut public = Vec::with_capacity(ir.public_inputs.len());
        let mut secret = Vec::with_capacity(ir.secret_inputs.len());
        for (kind, slots) in [("public", &ir.public_inputs), ("secret", &ir.secret_inputs)] {
            for (index, &expr) in slots.iter().enumerate() {
                let name = slot_name(ir, expr)
                    .ok_or(NamedWitnessError::SlotNotAVariable { kind, index })?;
                let value = self
                    .get(name)
                    .ok_or_else(|| NamedWitnessError::MissingInput {
                        name: name.to_owned(),
                        kind,
                        index,
                    })?;
                if kind == "public" {
                    public.push(value);
                } else {
                    secret.push(value);
                }
            }
        }
        // Anything left over names something this circuit does not declare.
        for (name, _) in &self.entries {
            if ir.variable_id(name).is_none() {
                return Err(NamedWitnessError::UnknownInput { name: name.clone() });
            }
        }
        Ok(WitnessValues { public, secret })
    }

    /// Resolves against `ir` and validates the constraints in one step.
    ///
    /// # Errors
    ///
    /// As [`NamedWitness::resolve`], or the first constraint failure as
    /// [`NamedWitnessError::Constraint`].
    pub fn resolve_and_check(
        &self,
        ir: &ConstraintSystem,
    ) -> Result<WitnessValues, NamedWitnessError> {
        let values = self.resolve(ir)?;
        values
            .check(ir)
            .map_err(|e| NamedWitnessError::Constraint(e.to_string()))?;
        Ok(values)
    }
}

/// A resolved witness: positional public and secret slices in IR declaration
/// order, exactly the shape [`witness::check`] and
/// [`ZkBackend::prove`](crate::ZkBackend::prove) consume.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WitnessValues {
    /// Public inputs, in declaration order.
    public: Vec<Scalar>,
    /// Secret inputs, in declaration order.
    secret: Vec<Scalar>,
}

impl WitnessValues {
    /// The public inputs, in the IR's declaration order.
    #[must_use]
    pub fn public(&self) -> &[Scalar] {
        &self.public
    }

    /// The secret inputs, in the IR's declaration order.
    #[must_use]
    pub fn secret(&self) -> &[Scalar] {
        &self.secret
    }

    /// Validates these values against `ir`'s constraints and declared types.
    ///
    /// # Errors
    /// As [`witness::check`].
    pub fn check(&self, ir: &ConstraintSystem) -> Result<(), crate::witness::WitnessError> {
        crate::witness::check(ir, &self.public, &self.secret)
    }
}

/// The name of the variable an input slot points at.
fn slot_name(ir: &ConstraintSystem, expr: usize) -> Option<&str> {
    match ir.exprs.get(expr) {
        Some(Expr::Var(var)) => ir.variables.get(*var).map(|info| info.name.as_str()),
        _ => None,
    }
}

/// Why a [`NamedWitness`] could not be resolved against a circuit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamedWitnessError {
    /// The witness names an input the circuit does not declare.
    UnknownInput {
        /// The unrecognized name.
        name: String,
    },
    /// A declared public or secret input has no value.
    MissingInput {
        /// The input that was not supplied.
        name: String,
        /// `"public"` or `"secret"`.
        kind: &'static str,
        /// The input's slot index.
        index: usize,
    },
    /// An input slot does not point at a named variable, so it has no name to
    /// key on. Only a malformed IR can cause this.
    SlotNotAVariable {
        /// `"public"` or `"secret"`.
        kind: &'static str,
        /// The offending slot index.
        index: usize,
    },
    /// The name-keyed witness resolved, but the values violate the circuit's
    /// constraints or declared input types.
    Constraint(String),
    /// A typed input value does not fit the IR's `i64` scalar model — for
    /// example a `u64` above `i64::MAX`. Refusing is the point: the IR models
    /// scalars as `i64`, and a silent `as` cast here would prove a *different*
    /// value than the caller supplied.
    ScalarOutOfRange {
        /// The input whose value did not fit. This is a compile-time parameter
        /// name, so it is borrowed for `'static`.
        name: &'static str,
    },
}

impl core::fmt::Display for NamedWitnessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownInput { name } => write!(
                f,
                "witness names `{name}`, which this circuit does not declare as an input"
            ),
            Self::MissingInput { name, kind, index } => {
                write!(f, "witness is missing {kind} input `{name}` (slot {index})")
            }
            Self::SlotNotAVariable { kind, index } => write!(
                f,
                "{kind} input slot {index} does not reference a named variable, so it cannot be \
                 keyed by name (is the IR malformed?)"
            ),
            Self::Constraint(msg) => write!(f, "witness violates the circuit: {msg}"),
            Self::ScalarOutOfRange { name } => write!(
                f,
                "input `{name}` does not fit the IR's i64 scalar model (the IR represents scalars \
                 as i64); supply a smaller value"
            ),
        }
    }
}

impl core::error::Error for NamedWitnessError {}
#[cfg(test)]
mod tests {
    use super::*;
    use tpt_axiom_ir::{ConstraintSystemBuilder, IntType};

    fn sample_ir() -> ConstraintSystem {
        let mut b = ConstraintSystemBuilder::new("transfer");
        let sender = b.public_input_typed("sender", IntType::U64);
        let receiver = b.public_input_typed("receiver", IntType::U64);
        let amount = b.secret_input_typed("amount", IntType::U64);
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let new_receiver = b.add(receiver, amount);
        let expected = b.add(receiver, amount);
        b.constrain_eq(new_receiver, expected);
        b.build()
    }

    #[test]
    fn resolution_is_by_name_not_position() {
        let ir = sample_ir();
        // Written in the opposite order from the circuit's declaration order;
        // the resolved slices still follow the IR.
        let witness = NamedWitness::new()
            .with("amount", 30)
            .with("receiver", 20)
            .with("sender", 50);
        let values = witness.resolve(&ir).expect("complete witness");
        assert_eq!(values.public(), &[50, 20]);
        assert_eq!(values.secret(), &[30]);
        assert!(values.check(&ir).is_ok());
    }

    #[test]
    fn an_impossible_value_fails_the_constraint_check() {
        let ir = sample_ir();
        // The reason this API exists: values written in the wrong order must
        // not become a proof of a different statement.
        let impossible = NamedWitness::new()
            .with("sender", 10)
            .with("receiver", 50)
            .with("amount", 30);
        assert!(impossible.resolve_and_check(&ir).is_err());
        let ok = NamedWitness::new()
            .with("sender", 50)
            .with("receiver", 50)
            .with("amount", 30);
        assert!(ok.resolve_and_check(&ir).is_ok());
    }

    #[test]
    fn a_missing_input_is_refused_rather_than_zero_filled() {
        let ir = sample_ir();
        assert_eq!(
            NamedWitness::new().with("sender", 50).resolve(&ir),
            Err(NamedWitnessError::MissingInput {
                name: String::from("receiver"),
                kind: "public",
                index: 1,
            })
        );
        assert_eq!(
            NamedWitness::new()
                .with("sender", 50)
                .with("receiver", 20)
                .resolve(&ir),
            Err(NamedWitnessError::MissingInput {
                name: String::from("amount"),
                kind: "secret",
                index: 0,
            })
        );
    }

    #[test]
    fn an_unknown_name_is_rejected_by_name() {
        let ir = sample_ir();
        let witness = NamedWitness::new()
            .with("sender", 50)
            .with("receiver", 20)
            .with("amount", 30)
            .with("amunt", 30);
        assert_eq!(
            witness.resolve(&ir),
            Err(NamedWitnessError::UnknownInput {
                name: String::from("amunt")
            })
        );
    }

    #[test]
    fn setting_a_name_twice_replaces_rather_than_duplicates() {
        let ir = sample_ir();
        let mut witness = NamedWitness::new()
            .with("sender", 50)
            .with("receiver", 20)
            .with("amount", 30);
        assert_eq!(witness.get("sender"), Some(50));
        witness.set("sender", 40);
        assert_eq!(witness.get("sender"), Some(40));
        assert_eq!(witness.len(), 3);
        let values = witness.resolve(&ir).expect("complete witness");
        assert_eq!(values.public(), &[40, 20]);
        // The replacement changed exactly the one value it named.
        assert!(values.check(&ir).is_ok());
    }

    #[test]
    fn an_empty_witness_names_the_first_missing_input() {
        let ir = sample_ir();
        assert!(NamedWitness::new().is_empty());
        assert_eq!(
            NamedWitness::new().resolve(&ir),
            Err(NamedWitnessError::MissingInput {
                name: String::from("sender"),
                kind: "public",
                index: 0,
            })
        );
    }

    #[test]
    fn a_malformed_slot_is_reported_not_panicked() {
        // An input slot pointing at a computed node (not a variable) has no
        // name to key on; only a malformed IR can produce this, and the
        // resolver says so instead of indexing past the end. Every earlier
        // slot is supplied, so the malformed one is what gets reached.
        let mut ir = sample_ir();
        let computed = ir.exprs.len();
        ir.exprs.push(Expr::Const(7));
        ir.var_exprs.push(computed);
        ir.public_inputs.push(computed);
        let complete = NamedWitness::new()
            .with("sender", 50)
            .with("receiver", 20)
            .with("amount", 30);
        assert_eq!(
            complete.resolve(&ir),
            Err(NamedWitnessError::SlotNotAVariable {
                kind: "public",
                index: 2
            })
        );
    }

    #[test]
    fn out_of_type_values_are_reported_by_the_check() {
        let mut b = ConstraintSystemBuilder::new("typed");
        b.public_input_typed("small", IntType::U8);
        let ir = b.build();
        let witness = NamedWitness::new().with("small", 300);
        // Resolution succeeds (the name exists); the declared-type check is
        // what objects.
        assert!(witness.resolve(&ir).is_ok());
        assert!(matches!(
            witness.resolve_and_check(&ir),
            Err(NamedWitnessError::Constraint(_))
        ));
    }
}
