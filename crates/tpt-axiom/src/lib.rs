//! The README doubles as the crate documentation: what you read on docs.rs
//! is what a newcomer reads in the repository, and every runnable snippet in
//! it is a doctest that fails CI if it drifts from the API.
#![doc = include_str!("../../../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "arkworks")]
pub use tpt_axiom_backend_arkworks;
#[cfg(feature = "halo2")]
pub use tpt_axiom_backend_halo2;
pub use tpt_axiom_core;
#[cfg(feature = "interop")]
pub use tpt_axiom_interop;
pub use tpt_axiom_ir;
pub use tpt_axiom_macros;
#[cfg(feature = "verify")]
pub use tpt_axiom_verify;
pub use tpt_axiom_zk;

pub mod audit;

/// The `tpt-axiom` prelude, matching the spec's `use tpt_axiom::prelude::*;`.
pub mod prelude {
    pub use tpt_axiom_core::{
        AbstentionReason, Bernoulli, BinaryDecision, Calibration, Categorical, Confidence,
        Decision, DecisionRecord, Distribution, Escalation, Evidence, Fuzzy, Hypotheses,
        MultiLabelDecision, Probability, Provenance, Ranking, Reproducibility, Score, Uncertain,
        Validate, classify_by_confidence, stats,
    };
    pub use tpt_axiom_ir::{ConstraintSystem, ConstraintSystemBuilder, Scalar};
    pub use tpt_axiom_macros::zk_provable;
    pub use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

    // Opt-in groups (see the crate features): one glob import picks up
    // everything that is compiled in.
    #[cfg(feature = "arkworks")]
    pub use tpt_axiom_backend_arkworks::{ArkworksBackend, ArkworksCircuit};
    #[cfg(feature = "halo2")]
    pub use tpt_axiom_backend_halo2::{Halo2Backend, Halo2Circuit};
    #[cfg(feature = "interop")]
    pub use tpt_axiom_interop::engine::{
        DecisionEngine, EngineError, EngineOutput, EngineVerdict, MultiLabelOutput,
        threshold_decision,
    };
    #[cfg(feature = "interop")]
    pub use tpt_axiom_interop::inference::InferenceSample;
    #[cfg(feature = "verify")]
    pub use tpt_axiom_verify::{Comparison, Mismatch, check_comparison, evaluate, normalize_all};

    pub use crate::audit::ProofCarriedDecision;
    pub use tpt_axiom_zk::{
        InputLayout, KeygenOptions, NamedWitness, ProofClaim, ProofEnvelope, ir_digest, keygen,
        prove_named, verify_claim,
    };
}
