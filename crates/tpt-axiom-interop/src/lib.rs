//! # tpt-axiom-interop
//!
//! Conversion interfaces between `tpt-axiom` and the rest of the TPT AI
//! ecosystem.
//!
//! * [`augur`] — conversions between [`tpt_augur_std::Dist`] (the
//!   [`tpt-augur`](https://github.com/tpt-solutions/tpt-augur)
//!   probabilistic-programming language's distribution family) and Axiom's
//!   `Fuzzy`/`Distribution`/`Uncertain`/`Bernoulli`, via exact closed-form
//!   moment matching. `tpt-augur-std` is a real crates.io dependency, so
//!   these are tested against the genuine type, not a mirror.
//! * [`inference`] — the boundary type ([`inference::InferenceSample`]) for
//!   the TPT inference runtimes (`tpt-gpu`, `tpt-local-ai`, `tpt-spark`).
//!   Those runtimes are not published yet; this module fixes the interface
//!   contract (sampled value + log-space weight → `Evidence`/`Score`) that
//!   their future feature-gated `From` impls will target.
//!
//! The crate is deliberately separate from `tpt-axiom-core`: ecosystem
//! coupling must never be a transitive dependency of the core types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod augur;
pub mod inference;

pub use tpt_axiom_core;
