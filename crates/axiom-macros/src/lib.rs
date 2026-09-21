//! # axiom-macros
//!
//! Procedural macros for `tpt-axiom`, currently the [`#[zk_provable]`] macro.
//!
//! [`#[zk_provable]`](macro@crate::zk_provable) analyzes an ordinary Rust
//! function and generates:
//!
//! 1. the original function unchanged (the "source of truth" Rust logic, used
//!    by the Phase 3 `tpt-telos` equivalence bridge), and
//! 2. a [`CircuitDefinition`] implementation that lowers the function's
//!    arithmetic constraints into a backend-agnostic
//!    [`axiom_ir::ConstraintSystem`].
//!
//! ```ignore
//! use axiom_macros::zk_provable;
//!
//! #[zk_provable(backend = "halo2")]
//! fn prove_balance_transfer(
//!     #[public] sender_balance: u64,
//!     #[public] receiver_balance: u64,
//!     #[secret] amount: u64,
//! ) {
//!     assert!(sender_balance >= amount);
//! }
//! ```
//!
//! ## Supported syntax
//!
//! * parameters annotated with `#[public]` (default) or `#[secret]`, whose
//!   types are integer primitives;
//! * straight-line `let` bindings built from `+`, `-`, `*`, parentheses, unary
//!   `-` and integer literals;
//! * `assert!(a >= b)` / `assert!(a <= b)` / `assert!(a > b)` /
//!   `assert!(a < b)` / `assert!(a == b)` comparisons;
//! * `assert_eq!(l, r)`; and
//! * an optional single integer `return`, which becomes a public output.
//!
//! Everything else — control flow, dynamic allocation, function/method calls,
//! trait objects, closures — produces a compile-time error.

use proc_macro::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

mod lower;

/// The attribute macro that turns a Rust function into a ZK circuit.
#[proc_macro_attribute]
pub fn zk_provable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match syn::parse::<ZkProvableArgs>(attr) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };
    let func = match syn::parse::<syn::ItemFn>(item) {
        Ok(func) => func,
        Err(err) => return err.to_compile_error().into(),
    };
    match lower::lower_function(&func, &args.backend) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parses `backend = "halo2"` configuration.
struct ZkProvableArgs {
    backend: String,
}

impl Parse for ZkProvableArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        if key != "backend" {
            return Err(syn::Error::new(
                key.span(),
                "zk_provable only supports `backend = \"...\"`",
            ));
        }
        input.parse::<Token![=]>()?;
        let value: LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after backend argument"));
        }
        Ok(ZkProvableArgs {
            backend: value.value(),
        })
    }
}