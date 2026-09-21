//! # tpt-axiom
//!
//! Probabilistic logic and native zero-knowledge state for Rust — an umbrella
//! crate re-exporting the `tpt-axiom` ecosystem behind the spec's
//! `use tpt_axiom::prelude::*;`.
//!
//! ## Probabilistic types
//!
//! ```
//! use tpt_axiom::prelude::*;
//!
//! let pos: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
//! let vel: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
//! let dt = 0.2_f64;
//!
//! let next_pos = pos + vel * dt; // uncertainty propagates automatically
//! assert!((next_pos.mean() - 10.9).abs() < 1e-9);
//! ```
//!
//! ## Zero-knowledge state
//!
//! ```
//! use tpt_axiom::prelude::*;
//!
//! #[zk_provable(backend = "halo2")]
//! fn prove_balance_transfer(
//!     #[public] sender_balance: u64,
//!     #[public] receiver_balance: u64,
//!     #[secret] amount: u64,
//! ) {
//!     assert!(sender_balance >= amount);
//! }
//!
//! let circuit = ProveBalanceTransfer;
//! let ir = circuit.build();
//! assert_eq!(ir.num_public(), 2);
//! ```
//!
//! See [`prelude`] for the intended glob import.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use axiom_core;
pub use axiom_ir;
pub use axiom_macros;
pub use axiom_zk;

/// The `tpt-axiom` prelude, matching the spec's `use tpt_axiom::prelude::*;`.
pub mod prelude {
    pub use axiom_core::{stats, Distribution, Fuzzy};
    pub use axiom_ir::{ConstraintSystem, ConstraintSystemBuilder, Scalar};
    pub use axiom_macros::zk_provable;
    pub use axiom_zk::{CircuitDefinition, ZkBackend};
}
