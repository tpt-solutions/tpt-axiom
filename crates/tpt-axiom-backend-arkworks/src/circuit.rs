//! The R1CS lowering of [`tpt_axiom_ir::ConstraintSystem`] onto arkworks.
//!
//! Every IR expression node becomes an arkworks witness variable tied to its
//! operands with `a·b = c` constraints (`Mul` is a genuine quadratic gate;
//! `Add`/`Sub`/`Neg` constrain against the constant one). Equality and zero
//! constraints become linear gates; `NonNegative` constraints and every named
//! input are enforced with bit decomposition — one boolean witness per bit
//! plus a linear sum, sized by that input's declared integer type — which is
//! what makes signed- and narrow-typed circuit semantics sound over the
//! BLS12-381 scalar field.

use ark_bls12_381::Fr;
use ark_ff::{One, Zero};
use ark_relations::r1cs::{
    ConstraintSynthesizer, ConstraintSystemRef, LinearCombination, SynthesisError, Variable,
};
use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr};

/// Bit width used to range-check named inputs and `NonNegative` constraints.
pub(crate) const DEFAULT_RANGE_BITS: u32 = 64;

/// arkworks-specific parameters decoded from `ZkBackend::generate_keys`'
/// `params` slice.
///
/// Encoding: `params[0]` = range bit width (0 or absent means the default of
/// 64, capped at 64). Groth16 needs no further parameters: setup is
/// circuit-specific.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArkworksParams {
    /// Range-check bit width for named inputs and `NonNegative` constraints.
    pub range_bits: u32,
}

impl Default for ArkworksParams {
    fn default() -> Self {
        Self {
            range_bits: DEFAULT_RANGE_BITS,
        }
    }
}

impl ArkworksParams {
    /// Decodes backend parameters; an empty slice yields the defaults.
    #[must_use]
    pub fn decode(params: &[u8]) -> Self {
        let mut decoded = Self::default();
        if let Some(bits) = params.first() {
            if *bits != 0 {
                decoded.range_bits = u32::from(*bits).min(64);
            }
        }
        decoded
    }

    /// Encodes parameters for round-tripping through the `ZkBackend` API.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        if self.range_bits == DEFAULT_RANGE_BITS {
            Vec::new()
        } else {
            vec![u8::try_from(self.range_bits).unwrap_or(u8::MAX).max(1)]
        }
    }
}

/// Encodes an `i64` scalar into the BLS12-381 scalar field (negatives as
/// field negation).
#[must_use]
pub fn encode_scalar(value: i64) -> Fr {
    encode_i128(i128::from(value))
}

/// Encodes an unsigned scalar into the BLS12-381 scalar field.
#[must_use]
pub fn encode_u64(value: u64) -> Fr {
    encode_i128(i128::from(value))
}

/// Encodes a signed `i128` scalar into the BLS12-381 scalar field (negatives
/// as field negation). The scalar field exceeds 254 bits, so every `i128`
/// value has a distinct image — no wraparound is possible.
#[must_use]
pub fn encode_i128(value: i128) -> Fr {
    let magnitude = Fr::from(value.unsigned_abs());
    if value < 0 { -magnitude } else { magnitude }
}

/// A [`ConstraintSystem`] lowered into arkworks R1CS, optionally carrying the
/// witness (the key-generation shape carries none).
#[derive(Clone, Debug)]
pub struct ArkworksCircuit {
    /// The IR this circuit was lowered from.
    pub ir: ConstraintSystem,
    /// Range-check bit width.
    pub range_bits: u32,
    public: Option<Vec<i64>>,
    secret: Option<Vec<i64>>,
}

impl ArkworksCircuit {
    /// Lowers an IR circuit without any witness (the key-generation shape).
    #[must_use]
    pub fn compile(ir: &ConstraintSystem, range_bits: u32) -> Self {
        Self {
            ir: ir.clone(),
            range_bits,
            public: None,
            secret: None,
        }
    }

    /// The attached public values, if this circuit carries a witness.
    #[must_use]
    pub fn public(&self) -> Option<&[i64]> {
        self.public.as_deref()
    }

    /// Attaches values without validating them against the IR.
    ///
    /// [`tpt_axiom_zk::witness::check`] (run by `ZkBackend::prove`) rejects
    /// violating witnesses up front; arkworks' own constraint synthesis is
    /// the second line of defense and fails proving naturally.
    ///
    /// # Errors
    /// Fails when the slice lengths disagree with the circuit's declared
    /// inputs.
    pub fn with_witness_unchecked(
        &self,
        public: Vec<i64>,
        secret: Vec<i64>,
    ) -> Result<Self, tpt_axiom_zk::witness::WitnessError> {
        if public.len() != self.ir.public_inputs.len() {
            return Err(tpt_axiom_zk::witness::WitnessError::Arity {
                kind: "public",
                expected: self.ir.public_inputs.len(),
                got: public.len(),
            });
        }
        if secret.len() != self.ir.secret_inputs.len() {
            return Err(tpt_axiom_zk::witness::WitnessError::Arity {
                kind: "secret",
                expected: self.ir.secret_inputs.len(),
                got: secret.len(),
            });
        }
        Ok(Self {
            ir: self.ir.clone(),
            range_bits: self.range_bits,
            public: Some(public),
            secret: Some(secret),
        })
    }

    /// Attaches validated values, producing the proving shape.
    ///
    /// # Errors
    /// Fails when the witness violates the IR (see
    /// [`tpt_axiom_zk::witness`]).
    pub fn with_witness(
        &self,
        public: Vec<i64>,
        secret: Vec<i64>,
    ) -> Result<Self, tpt_axiom_zk::witness::WitnessError> {
        tpt_axiom_zk::witness::check(&self.ir, &public, &secret)?;
        self.with_witness_unchecked(public, secret)
    }
}

fn lc(mut terms: Vec<(Fr, Variable)>) -> LinearCombination<Fr> {
    terms.retain(|(coeff, _)| !coeff.is_zero());
    LinearCombination(terms)
}

fn term(coeff: Fr, var: Variable) -> LinearCombination<Fr> {
    lc(vec![(coeff, var)])
}

fn var_lc(var: Variable) -> LinearCombination<Fr> {
    lc(vec![(Fr::one(), var)])
}

fn merge(
    mut l: LinearCombination<Fr>,
    sign: Fr,
    r: LinearCombination<Fr>,
) -> LinearCombination<Fr> {
    for (coeff, var) in r.0 {
        l.0.push((sign * coeff, var));
    }
    l
}

fn add_lc(l: LinearCombination<Fr>, r: LinearCombination<Fr>) -> LinearCombination<Fr> {
    merge(l, Fr::one(), r)
}

fn sub_lc(l: LinearCombination<Fr>, r: LinearCombination<Fr>) -> LinearCombination<Fr> {
    merge(l, -Fr::one(), r)
}

fn neg_lc(l: LinearCombination<Fr>) -> LinearCombination<Fr> {
    merge(lc(Vec::new()), -Fr::one(), l)
}

impl ConstraintSynthesizer<Fr> for ArkworksCircuit {
    #[allow(clippy::too_many_lines)] // one section per lowering rule
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let Self {
            ir,
            range_bits,
            public,
            secret,
        } = self;
        let range_bits = usize::try_from(range_bits).unwrap_or(0);
        let circuit_bits = range_bits;
        let witness = NodeWitness::compute(&ir, public.as_deref(), secret.as_deref());

        // arkworks requires input variables before witness variables, so the
        // named inputs are allocated in two dedicated passes.
        let mut var_of: Vec<Variable> = vec![Variable::One; ir.variables.len()];
        for id in &ir.public_inputs {
            if let Expr::Var(var) = ir.exprs[*id] {
                let value = witness.node(*id);
                var_of[var] = cs.new_input_variable(|| {
                    value
                        .map(encode_i128)
                        .ok_or(SynthesisError::AssignmentMissing)
                })?;
            }
        }
        for id in &ir.secret_inputs {
            if let Expr::Var(var) = ir.exprs[*id] {
                let value = witness.node(*id);
                var_of[var] = cs.new_witness_variable(|| {
                    value
                        .map(encode_i128)
                        .ok_or(SynthesisError::AssignmentMissing)
                })?;
            }
        }

        // A witness variable per operation node; the value is precomputed so
        // arkworks never has to solve for it.
        let mut node_lc: Vec<Option<LinearCombination<Fr>>> = vec![None; ir.exprs.len()];
        for (id, expr) in ir.exprs.iter().enumerate() {
            let combination = match expr {
                Expr::Var(var) => var_lc(var_of[*var]),
                Expr::Const(c) => term(encode_scalar(*c), Variable::One),
                Expr::Add(l, r) => {
                    let out = Self::alloc_node(&cs, witness.node(id))?;
                    let (l, r) = (
                        node_lc[*l].clone().expect("child"),
                        node_lc[*r].clone().expect("child"),
                    );
                    cs.enforce_constraint(var_lc(out), var_lc(Variable::One), add_lc(l, r))?;
                    var_lc(out)
                }
                Expr::Sub(l, r) => {
                    let out = Self::alloc_node(&cs, witness.node(id))?;
                    let (l, r) = (
                        node_lc[*l].clone().expect("child"),
                        node_lc[*r].clone().expect("child"),
                    );
                    cs.enforce_constraint(var_lc(out), var_lc(Variable::One), sub_lc(l, r))?;
                    var_lc(out)
                }
                Expr::Neg(n) => {
                    let out = Self::alloc_node(&cs, witness.node(id))?;
                    let n = node_lc[*n].clone().expect("child");
                    cs.enforce_constraint(var_lc(out), var_lc(Variable::One), neg_lc(n))?;
                    var_lc(out)
                }
                Expr::Mul(l, r) => {
                    let out = Self::alloc_node(&cs, witness.node(id))?;
                    let (l, r) = (
                        node_lc[*l].clone().expect("child"),
                        node_lc[*r].clone().expect("child"),
                    );
                    cs.enforce_constraint(l, r, var_lc(out))?;
                    var_lc(out)
                }
            };
            node_lc[id] = Some(combination);
        }

        for constraint in &ir.constraints {
            match *constraint {
                Constraint::Equal(l, r) => {
                    let (l, r) = (
                        node_lc[l].clone().expect("node"),
                        node_lc[r].clone().expect("node"),
                    );
                    cs.enforce_constraint(sub_lc(l, r), var_lc(Variable::One), lc(Vec::new()))?;
                }
                Constraint::Zero(e) => {
                    let e = node_lc[e].clone().expect("node");
                    cs.enforce_constraint(e, var_lc(Variable::One), lc(Vec::new()))?;
                }
                Constraint::NonNegative(e) => {
                    // Negative values deliberately wrap: the resulting bit
                    // sum then disagrees with the node's variable, so R1CS
                    // synthesis rejects the witness.
                    #[allow(
                        clippy::cast_sign_loss, // wrapping is the point
                        clippy::cast_possible_truncation // ...and so is truncation
                    )]
                    let checked = witness.node(e).map(|v| v as u64);
                    let source = node_lc[e].clone().expect("node");
                    range_check(&cs, range_bits, checked, &source)?;
                }
            }
        }

        // Range checks for every named input, honouring each variable's own
        // declared integer type. Signed types are shifted by `2^(bits-1)` into
        // `[0, 2^bits)`; unsigned types are checked directly against the input.
        for (var_id, info) in ir.variables.iter().enumerate() {
            let Some(expr_id) = ir.var_expr_id(var_id) else {
                continue;
            };
            let bits = usize::try_from(info.int_type.bits)
                .unwrap_or(64)
                .clamp(1, 64)
                .min(circuit_bits.max(1));
            let offset = info.int_type.signed_offset();
            let checked = witness.node(expr_id).map(|v| {
                if info.int_type.signed {
                    u64::try_from(v + i128::from(offset)).unwrap_or(u64::MAX)
                } else {
                    u64::try_from(v).unwrap_or(0)
                }
            });
            let input = node_lc[expr_id].clone().expect("input");
            let source = if info.int_type.signed {
                // checked value = input + offset
                merge(input, Fr::one(), term(encode_u64(offset), Variable::One))
            } else {
                input
            };
            range_check(&cs, bits, checked, &source)?;
        }

        cs.finalize();
        Ok(())
    }
}

impl ArkworksCircuit {
    fn alloc_node(
        cs: &ConstraintSystemRef<Fr>,
        value: Option<i128>,
    ) -> Result<Variable, SynthesisError> {
        cs.new_witness_variable(|| {
            value
                .map(encode_i128)
                .ok_or(SynthesisError::AssignmentMissing)
        })
    }
}

/// Bit-decomposition range check on `checked` (unknown at key generation),
/// tying the bit sum to `source` (the constrained node's linear combination).
fn range_check(
    cs: &ConstraintSystemRef<Fr>,
    range_bits: usize,
    checked: Option<u64>,
    source: &LinearCombination<Fr>,
) -> Result<(), SynthesisError> {
    let mut sum = lc(Vec::new());
    let mut power = Fr::one();
    let two = Fr::from(2u64);
    for bit_index in 0..range_bits {
        let bit = checked.map(|value| (value >> bit_index) & 1);
        let b = cs.new_witness_variable(|| {
            bit.map(encode_u64).ok_or(SynthesisError::AssignmentMissing)
        })?;
        // Booleanity: b * b = b.
        cs.enforce_constraint(var_lc(b), var_lc(b), var_lc(b))?;
        sum = merge(sum, Fr::one(), term(power, b));
        power *= two;
    }
    // Σ 2^i·b_i == source.
    cs.enforce_constraint(sum, var_lc(Variable::One), source.clone())
}

/// The value of every expression node, when a witness is available.
///
/// Exact `i128` evaluation ([`tpt_axiom_zk::witness::evaluate_nodes`]); an
/// overflowing node evaluates to `None`, which surfaces at synthesis as an
/// assignment failure rather than a silently wrapped assignment.
struct NodeWitness {
    values: Vec<Option<i128>>,
}

impl NodeWitness {
    fn compute(ir: &ConstraintSystem, public: Option<&[i64]>, secret: Option<&[i64]>) -> Self {
        Self {
            values: tpt_axiom_zk::witness::evaluate_nodes(ir, public, secret),
        }
    }

    fn node(&self, id: usize) -> Option<i128> {
        self.values.get(id).copied().flatten()
    }
}
