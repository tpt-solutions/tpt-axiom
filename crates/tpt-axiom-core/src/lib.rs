//! # axiom-core
//!
//! Probabilistic logic and uncertainty state for [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).
//!
//! This crate provides the foundational probabilistic types — [`Fuzzy<T>`] and
//! [`Distribution<T>`] — that wrap a mean and a variance and automatically
//! propagate uncertainty through operator overloading (error-propagation
//! algebra).  It is the Phase 1 deliverable scoped in `todo.md`.
//!
//! ## Todos (Phase 1)
//!
//! - [x] Define `Fuzzy<T>` struct (mean + variance)
//! - [x] Define `Distribution<T>` wrapper (Gaussian today, extensible later)
//! - [x] `Fuzzy::new(mean, variance)` constructor + accessors
//! - [x] `Add`/`Sub`/`Mul`/`Div` for `Fuzzy<T> ± Fuzzy<T>`, `Fuzzy<T> ± T`, and scalar variants
//! - [x] Statistical helpers (confidence interval, standard deviation, z-score)
//! - [x] Unit tests validating propagated variance against closed-form results
//! - [x] Property / Monte-Carlo cross-check tests
//! - [x] Kalman-filter / sensor-fusion example (`examples/`)
//!
//! > The probabilistic types in this crate are deliberately backend-independent:
//! > they depend on no AI vendor, model, inference engine, or network service.
//! > Zero-knowledge concerns live in `axiom-zk`, not here.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod distribution;
mod fuzzy;
mod operator;
pub mod traits;

#[cfg(test)]
mod monte_carlo_tests;

pub use distribution::Distribution;
pub use fuzzy::Fuzzy;
