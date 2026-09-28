//! # tpt-axiom-backend-halo2
//!
//! The `halo2` adapter for `tpt-axiom`.
//!
//! Implements [`tpt_axiom_zk::ZkBackend`] for the
//! [`halo2_proofs`](https://crates.io/crates/halo2_proofs) proving stack
//! (IPA/Pallas): it compiles the backend-agnostic
//! [`tpt_axiom_ir::ConstraintSystem`] produced by `#[zk_provable]` into a
//! `PLONKish` circuit ([`circuit`]), generates real proving and verifying keys,
//! and produces proofs that verify independently of the proving process.
//!
//! # Integer semantics
//!
//! The IR models scalars as `i64`. Over the Pallas base field the adapter
//! enforces that semantics with range checks: every named input is proven to
//! be a signed [`Halo2Params::range_bits`]-bit integer (default 64), and every
//! `NonNegative` constraint is proven to lie in `[0, 2^range_bits)`. Values
//! produced by `+`/`-`/`*` are exact field images of the integer DAG;
//! intermediate results that overflow `i64` make proving fail rather than
//! wrap.
//!
//! # Verifying-key portability
//!
//! halo2 0.3 does not expose verifying-key serialization, so a verifier
//! rebuilds the key deterministically from the circuit IR and the same `k`
//! (halo2 key generation consumes no randomness). See the crate's `examples/`
//! for a prove/verify pair that runs in separate processes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

use halo2_proofs::pasta::{Fp, vesta};
use halo2_proofs::plonk::{
    ProvingKey as Halo2ProvingKey, SingleVerifier, VerifyingKey as Halo2VerifyingKey, create_proof,
    keygen_pk, keygen_vk, verify_proof,
};
use halo2_proofs::poly::commitment::Params;
use halo2_proofs::transcript::{Blake2bRead, Blake2bWrite, Challenge255};
use rand::rngs::OsRng;

pub mod circuit;

pub use crate::circuit::{Halo2Circuit, Halo2Params, auto_k, encode_scalar, encode_u64};
pub use tpt_axiom_zk::witness::WitnessError;

pub use tpt_axiom_ir;
pub use tpt_axiom_zk;

/// The halo2 [`tpt_axiom_zk::ZkBackend`] implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct Halo2Backend;

impl Halo2Backend {
    /// The backend's stable identifier.
    pub const NAME: &'static str = "halo2";
}

/// Failures of the halo2 backend.
#[derive(Debug)]
pub enum Halo2Error {
    /// The public/secret witness slices disagree with the compiled circuit.
    Witness(WitnessError),
    /// Key generation, proving, or verification failed.
    Plonk(halo2_proofs::plonk::Error),
    /// The `params` byte string is malformed.
    InvalidParams(String),
}

impl fmt::Display for Halo2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Witness(e) => write!(f, "witness mismatch: {e}"),
            Self::Plonk(e) => write!(f, "halo2 plonk failure: {e:?}"),
            Self::InvalidParams(msg) => write!(f, "invalid halo2 params: {msg}"),
        }
    }
}

impl std::error::Error for Halo2Error {}

impl From<WitnessError> for Halo2Error {
    fn from(e: WitnessError) -> Self {
        Self::Witness(e)
    }
}

impl From<halo2_proofs::plonk::Error> for Halo2Error {
    fn from(e: halo2_proofs::plonk::Error) -> Self {
        Self::Plonk(e)
    }
}

/// Backend proving key: halo2's structured parameters plus the circuit's
/// proving key. Params ride along because `create_proof` needs them.
#[derive(Clone)]
pub struct Halo2ProvingMaterial {
    params: Params<vesta::Affine>,
    pk: Halo2ProvingKey<vesta::Affine>,
}

// Key fields are intentionally elided from Debug: halo2 keys hold megabytes
// of curve points with no compact human summary.
#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Halo2ProvingMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Halo2ProvingMaterial")
            .field("k", &self.params.k())
            .finish()
    }
}

/// Backend verifying key: halo2's structured parameters plus the circuit's
/// verifying key.
#[derive(Clone)]
pub struct Halo2VerifyingMaterial {
    params: Params<vesta::Affine>,
    vk: Halo2VerifyingKey<vesta::Affine>,
}

#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Halo2VerifyingMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Halo2VerifyingMaterial")
            .field("k", &self.params.k())
            .finish()
    }
}

/// A serialized halo2 proof (Blake2b-transcript bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Halo2Proof(pub Vec<u8>);

impl tpt_axiom_zk::ZkBackend for Halo2Backend {
    type Error = Halo2Error;
    type Circuit = Halo2Circuit;
    type ProvingKey = Halo2ProvingMaterial;
    type VerifyingKey = Halo2VerifyingMaterial;
    type Proof = Halo2Proof;

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn compile(&self, ir: &tpt_axiom_ir::ConstraintSystem) -> Result<Self::Circuit, Self::Error> {
        let params = Halo2Params::default();
        Ok(Halo2Circuit::compile(ir, params.range_bits, params.k))
    }

    fn generate_keys(
        &self,
        ir: &tpt_axiom_ir::ConstraintSystem,
        params: &[u8],
    ) -> Result<(Self::ProvingKey, Self::VerifyingKey), Self::Error> {
        let decoded = Halo2Params::decode(params);
        let k = if decoded.k == 0 {
            circuit::auto_k(ir, decoded.range_bits)
        } else {
            decoded.k
        };
        let halo2_params = Params::<vesta::Affine>::new(k);
        let shape = Halo2Circuit::compile(ir, decoded.range_bits, k);
        let vk = keygen_vk(&halo2_params, &shape)?;
        let pk = keygen_pk(&halo2_params, vk.clone(), &shape)?;
        Ok((
            Halo2ProvingMaterial {
                params: halo2_params.clone(),
                pk,
            },
            Halo2VerifyingMaterial {
                params: halo2_params,
                vk,
            },
        ))
    }

    fn prove(
        &self,
        circuit: &Self::Circuit,
        pk: &Self::ProvingKey,
        public: &[tpt_axiom_ir::Scalar],
        secret: &[tpt_axiom_ir::Scalar],
    ) -> Result<Self::Proof, Self::Error> {
        let witnessed = circuit.with_witness(public.to_vec(), secret.to_vec())?;
        tpt_axiom_zk::witness::check(&circuit.ir, public, secret)?;
        let instance: Vec<Fp> = public.iter().map(|&v| circuit::encode_scalar(v)).collect();

        let mut transcript =
            Blake2bWrite::<_, vesta::Affine, Challenge255<vesta::Affine>>::init(Vec::new());
        create_proof(
            &pk.params,
            &pk.pk,
            &[witnessed],
            &[&[instance.as_slice()]],
            OsRng,
            &mut transcript,
        )?;
        Ok(Halo2Proof(transcript.finalize()))
    }

    fn verify(
        &self,
        vk: &Self::VerifyingKey,
        public: &[tpt_axiom_ir::Scalar],
        proof: &Self::Proof,
    ) -> Result<bool, Self::Error> {
        let instance: Vec<Fp> = public.iter().map(|&v| circuit::encode_scalar(v)).collect();
        let mut transcript =
            Blake2bRead::<_, vesta::Affine, Challenge255<vesta::Affine>>::init(proof.0.as_slice());
        verify_proof(
            &vk.params,
            &vk.vk,
            SingleVerifier::new(&vk.params),
            &[&[instance.as_slice()]],
            &mut transcript,
        )
        .map(|()| true)
        .or_else(|e| match e {
            // A well-formed proof over the wrong public inputs fails its
            // opening; garbage bytes fail the transcript. Both are clean
            // "invalid" verdicts, not backend failures.
            halo2_proofs::plonk::Error::ConstraintSystemFailure
            | halo2_proofs::plonk::Error::Opening
            | halo2_proofs::plonk::Error::Transcript(_) => Ok(false),
            other => Err(Halo2Error::from(other)),
        })
    }
}
