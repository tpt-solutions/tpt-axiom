//! # tpt-axiom-backend-arkworks
//!
//! The `arkworks` adapter for `tpt-axiom`.
//!
//! Implements [`tpt_axiom_zk::ZkBackend`] for the
//! [arkworks](https://arkworks.rs) proving stack: it lowers the
//! backend-agnostic [`tpt_axiom_ir::ConstraintSystem`] produced by
//! `#[zk_provable]` into arkworks R1CS ([`circuit`]), runs Groth16
//! circuit-specific setup over BLS12-381, and produces proofs that verify
//! independently of the proving process.
//!
//! # Integer semantics
//!
//! The IR models scalars as `i64`. Over the BLS12-381 scalar field the
//! adapter enforces that semantics with bit-decomposition range checks: every
//! named input is proven to be a signed
//! [`ArkworksParams::range_bits`]-bit integer (default 64), and every
//! `NonNegative` constraint is proven to lie in `[0, 2^range_bits)`. Values
//! produced by `+`/`-`/`*` are exact field images of the integer DAG;
//! intermediate results that overflow `i64` make proving fail rather than
//! wrap.
//!
//! Unlike halo2 0.3, arkworks exposes canonical serialization
//! ([`ark_serialize::CanonicalSerialize`]) for proving keys, verifying keys
//! and proofs, so verifiers can consume shipped key material directly.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use ark_bls12_381::Bls12_381;
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use ark_std::rand::rngs::OsRng;
use core::fmt;

pub mod circuit;

pub use crate::circuit::{ArkworksCircuit, ArkworksParams, encode_scalar, encode_u64};
pub use tpt_axiom_zk::witness::WitnessError;

pub use tpt_axiom_ir;
pub use tpt_axiom_zk;

/// The arkworks [`tpt_axiom_zk::ZkBackend`] implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct ArkworksBackend;

impl ArkworksBackend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "arkworks";
}

/// Failures of the arkworks backend.
#[derive(Debug)]
pub enum ArkworksError {
    /// The public/secret witness slices disagree with the compiled circuit or
    /// violate its constraints.
    Witness(WitnessError),
    /// arkworks constraint synthesis, key generation, proving, or
    /// verification failed (Groth16's SNARK impl reports everything as
    /// `SynthesisError`).
    Synthesis(ark_relations::r1cs::SynthesisError),
}

impl fmt::Display for ArkworksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Witness(e) => write!(f, "witness mismatch: {e}"),
            Self::Synthesis(e) => write!(f, "arkworks failure: {e}"),
        }
    }
}

impl std::error::Error for ArkworksError {}

impl From<WitnessError> for ArkworksError {
    fn from(e: WitnessError) -> Self {
        Self::Witness(e)
    }
}

impl From<ark_relations::r1cs::SynthesisError> for ArkworksError {
    fn from(e: ark_relations::r1cs::SynthesisError) -> Self {
        Self::Synthesis(e)
    }
}

/// A Groth16 proof over BLS12-381.
#[derive(Clone, Debug)]
pub struct ArkworksProof(pub Proof<Bls12_381>);

impl tpt_axiom_zk::ZkBackend for ArkworksBackend {
    type Error = ArkworksError;
    type Circuit = ArkworksCircuit;
    type ProvingKey = ProvingKey<Bls12_381>;
    type VerifyingKey = VerifyingKey<Bls12_381>;
    type Proof = ArkworksProof;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn compile(&self, ir: &tpt_axiom_ir::ConstraintSystem) -> Result<Self::Circuit, Self::Error> {
        Ok(ArkworksCircuit::compile(
            ir,
            ArkworksParams::default().range_bits,
        ))
    }

    fn generate_keys(
        &self,
        ir: &tpt_axiom_ir::ConstraintSystem,
        params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error> {
        let decoded = ArkworksParams::decode(params);
        let shape = ArkworksCircuit::compile(ir, decoded.range_bits);
        let (pk, vk) = Groth16::<Bls12_381>::circuit_specific_setup(shape, &mut OsRng)?;
        Ok((pk, vk))
    }

    fn prove(
        &self,
        circuit: &Self::Circuit,
        pk: &Self::ProvingKey,
        public: &[tpt_axiom_ir::Scalar],
        secret: &[tpt_axiom_ir::Scalar],
    ) -> Result<Self::Proof, Self::Error> {
        tpt_axiom_zk::witness::check(&circuit.ir, public, secret)?;
        let witnessed = circuit.with_witness_unchecked(public.to_vec(), secret.to_vec())?;
        let proof = Groth16::<Bls12_381>::prove(pk, witnessed, &mut OsRng)?;
        Ok(ArkworksProof(proof))
    }

    fn verify(
        &self,
        vk: &Self::VerifyingKey,
        public: &[tpt_axiom_ir::Scalar],
        proof: &Self::Proof,
    ) -> Result<bool, Self::Error> {
        let inputs: Vec<_> = public.iter().map(|&v| encode_scalar(v)).collect();
        Groth16::<Bls12_381>::verify(vk, &inputs, &proof.0).map_err(ArkworksError::from)
    }
}

/// Canonical serialization helper: proving keys, verifying keys and proofs
/// all implement [`CanonicalSerialize`], so verifiers can consume shipped
/// key material directly.
///
/// # Errors
/// Propagates [`ark_serialize::SerializationError`].
pub fn to_bytes<T: CanonicalSerialize>(
    value: &T,
) -> Result<Vec<u8>, ark_serialize::SerializationError> {
    let mut out = Vec::with_capacity(value.compressed_size());
    value.serialize_compressed(&mut out)?;
    Ok(out)
}
