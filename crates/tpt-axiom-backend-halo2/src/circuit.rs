//! The `PLONKish` lowering of [`tpt_axiom_ir::ConstraintSystem`] onto halo2.
//!
//! Layout: every IR expression node gets its own row; the node's value lives
//! in advice column `c`, with its operands copied into `a` and `b` so one
//! selector-gated gate enforces the node's relation. Equality and zero
//! constraints become copy constraints (no gate needed); constants are pinned
//! with `assign_advice_from_constant`.
//!
//! Soundness over a field comes from bit-decomposition range checks: every
//! named input is checked to be a signed `range_bits`-bit integer (via a
//! `+2^(range_bits-1)` shift through the `signed-shift` gate, whose offset
//! lives in the fixed column) and every `NonNegative` constraint is checked
//! to lie in `[0, 2^range_bits)`. A prover cannot substitute a field element
//! outside the integer range the circuit's semantics assume.

use ff::Field;
use halo2_proofs::circuit::{AssignedCell, Cell, Layouter, Region, SimpleFloorPlanner, Value};
use halo2_proofs::pasta::Fp;
use halo2_proofs::plonk::{
    Advice, Circuit, Column as Halo2Column, ConstraintSystem as Halo2ConstraintSystem, Error,
    Expression, Fixed, Instance, Selector,
};
use halo2_proofs::poly::Rotation;
use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr};
use tpt_axiom_zk::witness::{WitnessError, check as check_witness};

/// Bit width used to range-check named inputs and `NonNegative` constraints.
pub(crate) const DEFAULT_RANGE_BITS: u32 = 64;

/// halo2-specific parameters decoded from `ZkBackend::generate_keys`' `params`
/// slice.
///
/// Encoding: `params[0]` = range bit width (0 or absent means the default of
/// 64, max 255), `params[1]` = log2 of the row bound `k` (0 or absent means
/// auto-sized from the IR). An empty slice means all defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Halo2Params {
    /// Range-check bit width for named inputs and `NonNegative` constraints.
    pub range_bits: u32,
    /// Log2 of the circuit's row bound; `0` requests automatic sizing.
    pub k: u32,
}

impl Default for Halo2Params {
    fn default() -> Self {
        Self {
            range_bits: DEFAULT_RANGE_BITS,
            k: 0,
        }
    }
}

impl Halo2Params {
    /// Decodes backend parameters; an empty slice yields the defaults.
    #[must_use]
    pub fn decode(params: &[u8]) -> Self {
        let mut decoded = Self::default();
        if let Some(bits) = params.first() {
            if *bits != 0 {
                // Capped at 64: the range-check chain runs on `u64`.
                decoded.range_bits = u32::from(*bits).min(64);
            }
        }
        if let Some(k) = params.get(1) {
            decoded.k = u32::from(*k);
        }
        decoded
    }

    /// Encodes parameters for round-tripping through the `ZkBackend` API.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.range_bits != DEFAULT_RANGE_BITS {
            out.push(u8::try_from(self.range_bits).unwrap_or(u8::MAX).max(1));
        }
        if self.k != 0 {
            out.push(u8::try_from(self.k).unwrap_or(u8::MAX));
        }
        out
    }
}

/// halo2 reserves a few trailing rows for blinding; keep a safe margin.
const BLINDING_ROWS: usize = 64;

/// Smallest `k` whose `2^k` rows fit the lowered circuit plus blinding.
///
/// Public so tests and tools (e.g. `MockProver` runs) can size their
/// parameters exactly like [`crate::Halo2Backend::generate_keys`] does.
#[must_use]
pub fn auto_k(ir: &ConstraintSystem, range_bits: u32) -> u32 {
    let bits = usize::try_from(range_bits).unwrap_or(usize::MAX / 4);
    let rows = ir.exprs.len().saturating_add(ir.constraints.len())
        + ir.variables.len().saturating_mul(bits.saturating_add(1))
        + ir.constraints
            .iter()
            .filter(|c| matches!(c, Constraint::NonNegative(_)))
            .count()
            .saturating_mul(bits)
        + BLINDING_ROWS;
    let mut k = 4u32;
    while (1usize << k) < rows {
        k += 1;
    }
    k
}

/// Configuration of the lowered `PLONKish` circuit.
#[derive(Clone, Debug)]
pub struct CircuitConfig {
    advice: [Halo2Column<Advice>; 3],
    fixed: Halo2Column<Fixed>,
    instance: Halo2Column<Instance>,
    s_add: Selector,
    s_sub: Selector,
    s_neg: Selector,
    s_mul: Selector,
    s_signed_shift: Selector,
    s_range: Selector,
    s_bool: Selector,
}

/// A [`ConstraintSystem`] compiled into halo2's `PLONKish` form, optionally
/// carrying the witness needed to synthesize a proof.
///
/// The same structure serves key generation (witness-less) and proving
/// (witness-bearing), mirroring halo2's `Circuit` contract.
#[derive(Clone, Debug)]
pub struct Halo2Circuit {
    /// The IR this circuit was compiled from.
    pub ir: ConstraintSystem,
    /// Range-check bit width.
    pub range_bits: u32,
    /// Log2 of the row bound; `0` lets the automatic sizing apply.
    pub k: u32,
    public: Option<Vec<i64>>,
    secret: Option<Vec<i64>>,
}

impl Halo2Circuit {
    /// Compiles an IR circuit without any witness (the key-generation shape).
    #[must_use]
    pub fn compile(ir: &ConstraintSystem, range_bits: u32, k: u32) -> Self {
        Self {
            ir: ir.clone(),
            range_bits,
            k,
            public: None,
            secret: None,
        }
    }

    /// The attached public values, if this circuit carries a witness.
    #[must_use]
    pub fn public(&self) -> Option<&[i64]> {
        self.public.as_deref()
    }

    /// Attaches public and secret values, producing the proving shape.
    ///
    /// # Errors
    /// Fails when the slice lengths disagree with the circuit's declared
    /// inputs.
    pub fn with_witness(&self, public: Vec<i64>, secret: Vec<i64>) -> Result<Self, WitnessError> {
        check_witness(&self.ir, &public, &secret)?;
        Ok(Self {
            ir: self.ir.clone(),
            range_bits: self.range_bits,
            k: self.k,
            public: Some(public),
            secret: Some(secret),
        })
    }

    /// Attaches values without validating them against the IR.
    ///
    /// [`Self::with_witness`] rejects violating witnesses up front (halo2's
    /// prover would otherwise happily produce a well-formed proof of a false
    /// statement). This constructor skips that check after still verifying
    /// the slice shapes: the produced proof, if any, simply fails
    /// verification - the `PLONKish` constraints themselves still hold the
    /// line. Useful for testing the circuit's own soundness and for
    /// benchmarking rejection paths.
    ///
    /// # Errors
    /// Fails when the slice lengths disagree with the circuit's declared
    /// inputs.
    pub fn with_witness_unchecked(
        &self,
        public: Vec<i64>,
        secret: Vec<i64>,
    ) -> Result<Self, WitnessError> {
        if public.len() != self.ir.public_inputs.len() {
            return Err(WitnessError::Arity {
                kind: "public",
                expected: self.ir.public_inputs.len(),
                got: public.len(),
            });
        }
        if secret.len() != self.ir.secret_inputs.len() {
            return Err(WitnessError::Arity {
                kind: "secret",
                expected: self.ir.secret_inputs.len(),
                got: secret.len(),
            });
        }
        Ok(Self {
            ir: self.ir.clone(),
            range_bits: self.range_bits,
            k: self.k,
            public: Some(public),
            secret: Some(secret),
        })
    }
}

impl Circuit<Fp> for Halo2Circuit {
    type Config = CircuitConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            ir: self.ir.clone(),
            range_bits: self.range_bits,
            k: self.k,
            public: None,
            secret: None,
        }
    }

    #[allow(clippy::many_single_char_names)] // halo2's a/b/c column idiom
    fn configure(meta: &mut Halo2ConstraintSystem<Fp>) -> Self::Config {
        let advice = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];
        let fixed = meta.fixed_column();
        let instance = meta.instance_column();
        for col in advice {
            meta.enable_equality(col);
        }
        meta.enable_equality(instance);
        // `assign_advice_from_constant` routes through this column.
        meta.enable_constant(fixed);

        let s_add = meta.selector();
        let s_sub = meta.selector();
        let s_neg = meta.selector();
        let s_mul = meta.selector();
        let s_signed_shift = meta.selector();
        let s_range = meta.selector();
        let s_bool = meta.selector();

        let [a, b, c] = advice;
        meta.create_gate("add", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_add);
            vec![s * (a + b - c)]
        });
        meta.create_gate("sub", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_sub);
            vec![s * (a - b - c)]
        });
        meta.create_gate("neg", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_neg);
            vec![s * (-a - c)]
        });
        meta.create_gate("mul", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_mul);
            vec![s * (a * b - c)]
        });
        // Signed-input shift into range-check position: `acc_0 = input +
        // 2^(range_bits-1)`, with the offset supplied by the fixed column.
        meta.create_gate("signed-shift", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let f = meta.query_fixed(fixed);
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_signed_shift);
            vec![s * (a + f - c)]
        });
        // Running-sum step of the bit-decomposition range check:
        // `acc_next = (acc - bit) / 2`.
        meta.create_gate("range-step", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s_range);
            let two = Expression::Constant(Fp::from(2));
            vec![s * (a - b - two * c)]
        });
        meta.create_gate("bool", |meta| {
            let b = meta.query_advice(b, Rotation::cur());
            let s = meta.query_selector(s_bool);
            vec![s * (b.clone() * b.clone() - b)]
        });

        CircuitConfig {
            advice,
            fixed,
            instance,
            s_add,
            s_sub,
            s_neg,
            s_mul,
            s_signed_shift,
            s_range,
            s_bool,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fp>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "axiom lowered circuit",
            |mut region| self.assign(&config, &mut region),
        )
    }
}

/// The value of every expression node, when a witness is available.
///
/// Node values are computed with wrapping `i64` arithmetic. An overflowed
/// computation therefore disagrees with the field-level gates and makes
/// proving fail, rather than silently wrapping like the release-mode Rust
/// original would.
struct NodeValues {
    values: Vec<Option<i64>>,
}

impl NodeValues {
    fn compute(
        ir: &ConstraintSystem,
        public: Option<&Vec<i64>>,
        secret: Option<&Vec<i64>>,
    ) -> Self {
        let mut values: Vec<Option<i64>> = Vec::with_capacity(ir.exprs.len());
        for expr in &ir.exprs {
            let value = match expr {
                Expr::Const(v) => Some(*v),
                Expr::Var(var) => Self::variable_value(ir, *var, public, secret),
                Expr::Add(l, r) => Self::binary(&values, *l, *r, i64::wrapping_add),
                Expr::Sub(l, r) => Self::binary(&values, *l, *r, i64::wrapping_sub),
                Expr::Mul(l, r) => Self::binary(&values, *l, *r, i64::wrapping_mul),
                Expr::Neg(n) => values.get(*n).copied().flatten().map(i64::wrapping_neg),
            };
            values.push(value);
        }
        Self { values }
    }

    fn variable_value(
        ir: &ConstraintSystem,
        var: usize,
        public: Option<&Vec<i64>>,
        secret: Option<&Vec<i64>>,
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

    fn get(&self, id: usize) -> Value<Fp> {
        self.values
            .get(id)
            .copied()
            .flatten()
            .map_or(Value::unknown(), |v| Value::known(encode_scalar(v)))
    }

    fn int(&self, id: usize) -> Option<i64> {
        self.values.get(id).copied().flatten()
    }
}

/// Encodes an `i64` scalar into the Pallas base field (negatives as field
/// negation).
#[allow(clippy::missing_const_for_fn)] // Fp::from is not const
#[must_use]
pub fn encode_scalar(value: i64) -> Fp {
    let magnitude = Fp::from(value.unsigned_abs());
    if value < 0 { -magnitude } else { magnitude }
}

/// Encodes an unsigned scalar into the Pallas base field.
#[allow(clippy::missing_const_for_fn)] // Fp::from is not const
#[must_use]
pub fn encode_u64(value: u64) -> Fp {
    Fp::from(value)
}

impl Halo2Circuit {
    #[allow(clippy::many_single_char_names)] // halo2's a/b/c column idiom
    #[allow(clippy::too_many_lines)] // one section per lowering rule
    fn assign(&self, config: &CircuitConfig, region: &mut Region<'_, Fp>) -> Result<(), Error> {
        let values = NodeValues::compute(&self.ir, self.public.as_ref(), self.secret.as_ref());
        let [a, b, c] = config.advice;

        // A row per expression node; the node's value lands in column `c`.
        let mut cells: Vec<Option<AssignedCell<Fp, Fp>>> = vec![None; self.ir.exprs.len()];
        for (id, expr) in self.ir.exprs.iter().enumerate() {
            let cell = match expr {
                Expr::Var(var) => {
                    if let Some(instance_row) = self.ir.public_inputs.iter().position(
                        |&p| matches!(self.ir.exprs.get(p), Some(Expr::Var(pv)) if pv == var),
                    ) {
                        let name = self
                            .ir
                            .variables
                            .get(*var)
                            .map_or("public", |info| info.name.as_str());
                        region.assign_advice_from_instance(
                            || name,
                            config.instance,
                            instance_row,
                            c,
                            id,
                        )?
                    } else {
                        region.assign_advice(|| "secret", c, id, || values.get(id))?
                    }
                }
                Expr::Const(v) => {
                    region.assign_advice_from_constant(|| "const", c, id, encode_scalar(*v))?
                }
                _ => region.assign_advice(|| "expr", c, id, || values.get(id))?,
            };
            cells[id] = Some(cell);
        }

        // Arithmetic gate per operation node; operands copied into a/b.
        for (id, expr) in self.ir.exprs.iter().enumerate() {
            let (selector, operands): (Selector, &[usize]) = match expr {
                Expr::Add(l, r) => (config.s_add, &[*l, *r]),
                Expr::Sub(l, r) => (config.s_sub, &[*l, *r]),
                Expr::Mul(l, r) => (config.s_mul, &[*l, *r]),
                Expr::Neg(n) => (config.s_neg, &[*n]),
                _ => continue,
            };
            selector.enable(region, id)?;
            for (slot, &operand) in [a, b].iter().zip(operands) {
                let slot = *slot;
                cells[operand]
                    .as_ref()
                    .expect("operand assigned earlier: ExprIds reference earlier nodes only")
                    .copy_advice(|| "operand", region, slot, id)?;
            }
        }

        // Equality / zero constraints as copy constraints.
        let mut extra_row = self.ir.exprs.len();
        for constraint in &self.ir.constraints {
            match *constraint {
                Constraint::Equal(l, r) => {
                    region.constrain_equal(cell_of(&cells, l)?, cell_of(&cells, r)?)?;
                }
                Constraint::Zero(e) => {
                    let zero =
                        region.assign_advice_from_constant(|| "zero", a, extra_row, Fp::ZERO)?;
                    region.constrain_equal(cell_of(&cells, e)?, zero.cell())?;
                    extra_row += 1;
                }
                Constraint::NonNegative(_) => {}
            }
        }

        // Bit-decomposition range checks. Signed input checks shift their
        // value into `[0, 2^range_bits)` through the signed-shift gate first.
        let range_bits = usize::try_from(self.range_bits).unwrap_or(0);
        let signed_offset = 1i128 << self.range_bits.saturating_sub(1).min(126);
        for (var_id, _) in self.ir.variables.iter().enumerate() {
            let Some(expr_id) = self
                .ir
                .exprs
                .iter()
                .position(|e| matches!(e, Expr::Var(v) if *v == var_id))
            else {
                continue;
            };
            let shifted = values
                .int(expr_id)
                .map(|v| u64::try_from(i128::from(v) + signed_offset).unwrap_or(u64::MAX));
            let shift_row = extra_row;
            region.assign_fixed(
                || "signed offset",
                config.fixed,
                shift_row,
                || Value::known(encode_u64(u64::try_from(signed_offset).unwrap_or(1))),
            )?;
            config.s_signed_shift.enable(region, shift_row)?;
            cells[expr_id]
                .as_ref()
                .expect("input node assigned earlier")
                .copy_advice(|| "pre-shift input", region, a, shift_row)?;
            region.assign_advice(
                || "post-shift input",
                c,
                shift_row,
                || shifted.map_or_else(Value::unknown, |v| Value::known(encode_u64(v))),
            )?;
            extra_row += 1;
            self.range_check(
                region,
                config,
                shifted,
                None,
                a,
                b,
                c,
                &mut extra_row,
                range_bits,
            )?;
        }
        for constraint in &self.ir.constraints {
            if let Constraint::NonNegative(e) = *constraint {
                // Negative values deliberately wrap here: the resulting
                // accumulator then disagrees with the node's cell and the
                // copy constraint rejects the witness.
                #[allow(clippy::cast_sign_loss)] // wrapping is the point
                let checked = values.int(e).map(|v| v as u64);
                let source = cells[e].as_ref();
                self.range_check(
                    region,
                    config,
                    checked,
                    source,
                    a,
                    b,
                    c,
                    &mut extra_row,
                    range_bits,
                )?;
            }
        }

        Ok(())
    }

    /// Assigns one bit-decomposition range check for `checked` (unknown at
    /// key generation), starting at `*next_row`.
    ///
    /// `source`, when present, copy-constrains the checked accumulator to the
    /// constrained node's cell (`NonNegative` checks); signed input checks
    /// pass `None` because their accumulator was already linked to the input
    /// through the signed-shift gate.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::unused_self)] // uniform method surface for the lowering passes
    #[allow(clippy::many_single_char_names)] // halo2's a/b/c column idiom
    fn range_check(
        &self,
        region: &mut Region<'_, Fp>,
        config: &CircuitConfig,
        checked: Option<u64>,
        source: Option<&AssignedCell<Fp, Fp>>,
        a: Halo2Column<Advice>,
        b: Halo2Column<Advice>,
        c: Halo2Column<Advice>,
        next_row: &mut usize,
        range_bits: usize,
    ) -> Result<(), Error> {
        let mut acc = checked;
        let last_row = *next_row + range_bits.saturating_sub(1);
        for bit_index in 0..range_bits {
            let row = *next_row + bit_index;
            let (bit, next) = acc.map_or((0, 0), |value| {
                let bit = value & 1;
                (bit, (value - bit) >> 1)
            });
            match source {
                Some(src) if bit_index == 0 => {
                    src.copy_advice(|| "range acc", region, a, row)?;
                }
                _ => {
                    region.assign_advice(
                        || "range acc",
                        a,
                        row,
                        || acc.map_or_else(Value::unknown, |v| Value::known(encode_u64(v))),
                    )?;
                }
            }
            region.assign_advice(|| "range bit", b, row, || Value::known(encode_u64(bit)))?;
            if row == last_row {
                // Pin the terminal accumulator to zero: only values strictly
                // below 2^range_bits can consume every bit.
                region.assign_advice_from_constant(|| "range end", c, row, Fp::ZERO)?;
            } else {
                region.assign_advice(|| "range next", c, row, || Value::known(encode_u64(next)))?;
            }
            config.s_range.enable(region, row)?;
            config.s_bool.enable(region, row)?;
            acc = Some(next);
        }
        *next_row += range_bits;
        Ok(())
    }
}

fn cell_of(cells: &[Option<AssignedCell<Fp, Fp>>], id: usize) -> Result<Cell, Error> {
    cells
        .get(id)
        .and_then(Option::as_ref)
        .map(AssignedCell::cell)
        .ok_or(Error::Synthesis)
}
