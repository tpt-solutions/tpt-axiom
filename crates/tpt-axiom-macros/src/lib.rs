//! # axiom-macros
//!
//! Procedural macros for tpt-axiom.  The headline macro is
//! `#[zk_provable]`, which parses the annotated Rust function's AST and lowers
//! it to the shared [`axiom-ir`](https://github.com/tpt-solutions/tpt-axiom)
//! arithmetic intermediate representation, generating a circuit definition that
//! any [`ZkBackend`](https://github.com/tpt-solutions/tpt-axiom) can consume.
//!
//! ## Phase 2 scope
//!
//! - Parse annotated function AST (`syn`/`quote`)
//! - Distinguish `pub` (public input), plain (public output/local), and
//!   `secret` (witness) parameters
//! - Translate `assert!` / `assert_eq!` into IR constraints
//! - Emit compiler errors for unsupported constructs (dynamic-bound loops,
//!   heap allocation, trait objects)
//! - `backend = "halo2"` attribute to select the target at compile time
//!
//! > **Safety note:** this crate contains no `unsafe` code.  It is a pure
//! > AST-transformation layer; all mathematical/statistical logic lives in
//! > `axiom-core`.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
