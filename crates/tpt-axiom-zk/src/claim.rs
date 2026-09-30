//! The verification boundary between probabilistic computation and
//! zero-knowledge proof.
//!
//! A [`ProofClaim`] is the *statement* "this named circuit holds for these
//! public inputs" together with an opaque proof of it. It is deliberately
//! data-only: the intelligence types in `tpt-axiom-core` never reference it,
//! so cryptographic mechanisms remain opt-in and cannot contaminate the core
//! type system. Verification runs through any [`ZkBackend`] — the claim never
//! sees keys or circuits itself.
//!
//! The claim additionally *commits* to the exact [`ConstraintSystem`] it was
//! produced against ([`ir_digest`], a SHA-256 hash of the IR's canonical
//! encoding). [`ProofClaim::verify_with`] recomputes that digest from the IR
//! the verifier holds and refuses the claim (clean `Ok(false)`) on any
//! mismatch — a proof can never be replayed against a different circuit, for
//! example one with an extra weakening constraint or a widened input type.

use alloc::string::String;
use alloc::vec::Vec;

use sha2::{Digest, Sha256};
use tpt_axiom_ir::{Constraint, ConstraintSystem, Expr};

use crate::ZkBackend;
use tpt_axiom_ir::Scalar;

/// The SHA-256 digest of an IR's canonical encoding: the commitment a
/// [`ProofClaim`] carries and [`ProofClaim::verify_with`] re-checks.
///
/// Every part of the circuit that could change what a proof means is hashed:
/// the name, each variable's name/visibility/declared type, the full
/// expression table, the variable→expression map, the public/secret slot
/// lists and the constraints.
#[must_use]
pub fn ir_digest(ir: &ConstraintSystem) -> [u8; 32] {
    let mut h = Sha256::new();
    feed_str(&mut h, &ir.name);
    feed_usize(&mut h, ir.variables.len());
    for var in &ir.variables {
        feed_str(&mut h, &var.name);
        h.update([match var.visibility {
            tpt_axiom_ir::Visibility::Public => 0,
            tpt_axiom_ir::Visibility::Secret => 1,
        }]);
        h.update(var.int_type.bits.to_le_bytes());
        h.update([u8::from(var.int_type.signed)]);
    }
    feed_usize(&mut h, ir.exprs.len());
    for expr in &ir.exprs {
        match *expr {
            Expr::Const(v) => {
                h.update([0]);
                h.update(v.to_le_bytes());
            }
            Expr::Var(v) => {
                h.update([1]);
                feed_usize(&mut h, v);
            }
            Expr::Add(l, r) => {
                h.update([2]);
                feed_usize(&mut h, l);
                feed_usize(&mut h, r);
            }
            Expr::Sub(l, r) => {
                h.update([3]);
                feed_usize(&mut h, l);
                feed_usize(&mut h, r);
            }
            Expr::Mul(l, r) => {
                h.update([4]);
                feed_usize(&mut h, l);
                feed_usize(&mut h, r);
            }
            Expr::Neg(n) => {
                h.update([5]);
                feed_usize(&mut h, n);
            }
        }
    }
    for id in ir.var_exprs.iter().chain(&ir.public_inputs).chain(&ir.secret_inputs) {
        feed_usize(&mut h, *id);
    }
    for c in &ir.constraints {
        match *c {
            Constraint::Zero(e) => {
                h.update([0]);
                feed_usize(&mut h, e);
            }
            Constraint::Equal(l, r) => {
                h.update([1]);
                feed_usize(&mut h, l);
                feed_usize(&mut h, r);
            }
            Constraint::NonNegative(e) => {
                h.update([2]);
                feed_usize(&mut h, e);
            }
        }
    }
    h.finalize().into()
}

fn feed_str(h: &mut Sha256, s: &str) {
    feed_usize(h, s.len());
    h.update(s.as_bytes());
}

fn feed_usize(h: &mut Sha256, v: usize) {
    h.update((v as u64).to_le_bytes());
}

/// A portable, verifiable statement: `circuit` holds for `publics`, witnessed
/// by an opaque backend-specific `proof`, committed to the exact IR it was
/// proven against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofClaim<B: ZkBackend> {
    circuit: String,
    binding: [u8; 32],
    publics: Vec<Scalar>,
    proof: B::Proof,
}

impl<B: ZkBackend> ProofClaim<B> {
    /// Records a claim that `circuit` holds for `publics` with `proof`,
    /// committing to `ir` (the circuit definition the proof was produced
    /// against).
    #[must_use]
    pub fn new(
        circuit: impl Into<String>,
        ir: &ConstraintSystem,
        publics: &[Scalar],
        proof: B::Proof,
    ) -> Self {
        Self {
            circuit: circuit.into(),
            binding: ir_digest(ir),
            publics: publics.to_vec(),
            proof,
        }
    }

    /// The claimed circuit name.
    #[must_use]
    pub fn circuit(&self) -> &str {
        &self.circuit
    }

    /// The IR commitment this claim carries (see [`ir_digest`]).
    #[must_use]
    pub const fn binding(&self) -> [u8; 32] {
        self.binding
    }

    /// The claimed public inputs, in the circuit's declaration order.
    #[must_use]
    pub fn publics(&self) -> &[Scalar] {
        &self.publics
    }

    /// Verifies this claim with `backend` against the circuit's verifying
    /// key, first checking the claim's IR commitment against `ir` — the
    /// circuit definition the verifier believes it is checking. A claim
    /// produced against a different circuit (different constraints, types,
    /// even different expression structure) is rejected as `Ok(false)` before
    /// the backend is consulted.
    ///
    /// This is the one-way boundary: probabilistic code can hold a claim and
    /// hand it to any backend for checking, without the claim (or anything
    /// upstream of it) depending on the backend's types.
    ///
    /// # Errors
    /// Backend-specific verification failure. A malformed proof is a clean
    /// `Ok(false)` where the backend can express that.
    pub fn verify_with(
        &self,
        backend: &B,
        vk: &B::VerifyingKey,
        ir: &ConstraintSystem,
    ) -> Result<bool, B::Error> {
        if ir_digest(ir) != self.binding {
            return Ok(false);
        }
        backend.verify(vk, &self.publics, &self.proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_axiom_ir::ConstraintSystemBuilder;

    fn sample_ir() -> ConstraintSystem {
        let mut b = ConstraintSystemBuilder::new("claim_me");
        let x = b.public_input("x");
        let y = b.secret_input("y");
        let sum = b.add(x, y);
        b.constrain_zero(sum);
        b.build()
    }

    #[test]
    fn digest_is_deterministic_and_sensitive() {
        let ir = sample_ir();
        assert_eq!(ir_digest(&ir), ir_digest(&sample_ir()));
        // A different constraint changes the digest.
        let mut other = sample_ir();
        other.constraints.clear();
        assert_ne!(ir_digest(&ir), ir_digest(&other));
        // So does a declared type.
        let mut other = sample_ir();
        other.variables[0].int_type = tpt_axiom_ir::IntType::U8;
        assert_ne!(ir_digest(&ir), ir_digest(&other));
        // So does an otherwise-unused change to an expression node.
        let mut other = sample_ir();
        let sum = other
            .exprs
            .iter()
            .position(|e| matches!(e, Expr::Add(_, _)))
            .expect("sum node");
        other.exprs[sum] = Expr::Sub(0, 1);
        assert_ne!(ir_digest(&ir), ir_digest(&other));
    }
}
