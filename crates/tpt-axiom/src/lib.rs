//! # tpt-axiom
//!
//! Probabilistic Logic and Native Zero-Knowledge State for Rust.
//!
//! ```
//! use tpt_axiom::prelude::*;
//!
//! let pos: Fuzzy<f64> = Fuzzy::new(10.5, 0.5);
//! let vel: Fuzzy<f64> = Fuzzy::new(2.0, 0.1);
//! let dt = 1.0;
//!
//! // Ordinary `+`/`*` on `Fuzzy<T>` propagates variance automatically.
//! let next_pos = pos + (vel * dt);
//! assert!(next_pos.variance() > pos.variance());
//! ```
//!
//! See [`spec.txt`](https://github.com/tpt-solutions/tpt-axiom/blob/master/spec.txt)
//! for the full design and `todo.md` for the current implementation phase.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// The `tpt-axiom` prelude. `use tpt_axiom::prelude::*;` brings the
/// probabilistic types into scope.
pub mod prelude {
    pub use axiom_core::{Distribution, Fuzzy};
}

pub use axiom_core::{Distribution, Fuzzy};
