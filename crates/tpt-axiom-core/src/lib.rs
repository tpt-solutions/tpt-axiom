//! # tpt-axiom-core
//!
//! Probabilistic and uncertainty types for `tpt-axiom`.
//!
//! This crate provides [`Fuzzy<T>`] (a value plus a variance, with automatic
//! error propagation through the standard arithmetic operators) and
//! [`Distribution<T>`] (a more general probability-distribution wrapper).
//!
//! Every arithmetic operation on a [`Fuzzy<T>`] produces a new [`Fuzzy<T>`]
//! whose mean and variance are derived from the first-order (Gaussian) error
//! propagation rules. This makes Kalman filters, Monte Carlo simulations and
//! sensor fusion read like ordinary floating point arithmetic.
//!
//! ```
//! use tpt_axiom_core::Fuzzy;
//!
//! let pos: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
//! let vel: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
//! let dt = 0.2_f64;
//!
//! let next_pos = pos + vel * dt; // uncertainty propagates automatically
//! assert!((next_pos.mean() - 10.9).abs() < 1e-9);
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rust_2018_idioms)]

extern crate alloc;

mod decision_types;
mod distribution;
mod fuzzy;
mod intelligence;
mod quants;

pub use crate::decision_types::{
    BinaryDecision, Calibration, DecisionRecord, Hypotheses, MultiLabelDecision, Ranking,
};
pub use crate::distribution::{Distribution, InvalidVariance};
pub use crate::fuzzy::{FuseError, Fuzzy};
pub use crate::intelligence::{
    AbstentionReason, Bernoulli, Categorical, CategoricalError, Confidence, Decision,
    EmptyCategorical, Escalation, Evidence, InvalidConfidence, InvalidEvidenceWeight,
    InvalidProbability, Probability, Provenance, Reproducibility, Score, Uncertain, Validate,
    ValidationError, classify_by_confidence,
};

/// Statistical helpers available without importing sub-modules.
pub mod stats {
    pub use crate::quants::{erf, erfc, norm_cdf, norm_ppf};
}
