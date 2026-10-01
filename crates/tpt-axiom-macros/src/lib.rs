//! # tpt-axiom-macros
//!
//! Procedural macros for `tpt-axiom`, currently the [`#[zk_provable]`] macro.
//!
//! [`#[zk_provable]`](macro@crate::zk_provable) analyzes an ordinary Rust
//! function and generates:
//!
//! 1. the original function unchanged (the "source of truth" Rust logic,
//!    cross-checked against the IR by `tpt-axiom-verify`'s equivalence
//!    tests),
//! 2. a `CircuitDefinition` implementation (from `tpt-axiom-zk`) that lowers
//!    the function's
//!    arithmetic constraints into a backend-agnostic
//!    `tpt_axiom_ir::ConstraintSystem`, and
//! 3. a typed `<Fn>Inputs` struct with one field per parameter at that
//!    parameter's declared Rust type, whose `named()` method produces a
//!    `NamedWitness` for `tpt_axiom_zk::prove_named`.
//!
//! ```ignore
//! use tpt_axiom_macros::zk_provable;
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
//! // The macro also generates `ProveBalanceTransferInputs`:
//! let inputs = ProveBalanceTransferInputs::new(50, 20, 30);
//! let witness = inputs.named()?;
//! ```
//!
//! The typed inputs matter because the positional witness slices a backend
//! takes are easy to transpose: two adjacent `u64` fields swapped in the wrong
//! order yield a well-formed witness for a *different* statement. Keying values
//! by name, and refusing to resolve a name map that is not exactly the
//! circuit's input set, closes that hole.
//!
//! ```ignore
//! use tpt_axiom_macros::zk_provable;
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
use proc_macro2::Span;
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
    match lower::lower_function(&func, &args.backend, args.register) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parses `#[zk_provable(backend = "...", register)]` configuration.
struct ZkProvableArgs {
    backend: String,
    /// Whether to emit the link-time circuit registration.
    register: bool,
}

impl Parse for ZkProvableArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut backend: Option<String> = None;
        let mut register = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "backend" => {
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    backend = Some(validate_backend(&value)?);
                }
                "register" => register = true,
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `#[zk_provable]` option `{other}`; supported options are \
                             `backend = \"...\"` and `register` (link-time circuit registry)"
                        ),
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        let Some(backend) = backend else {
            return Err(syn::Error::new(
                Span::call_site(),
                "#[zk_provable] requires `backend = \"...\"` (e.g. `backend = \"halo2\"`)",
            ));
        };
        Ok(Self { backend, register })
    }
}

/// Validates the backend selector, suggesting the fix for a near miss.
fn validate_backend(value: &LitStr) -> syn::Result<String> {
    let backend = value.value();
    if backend.is_empty() {
        return Err(syn::Error::new(
            value.span(),
            "`backend` must not be empty (e.g. `backend = \"halo2\"`)",
        ));
    }
    // Third-party backends are allowed (the string only feeds the
    // `CircuitDefinition::backend` selector), but a near-miss of a known
    // backend is almost certainly a typo — say so, with the fix.
    let known = ["halo2", "arkworks", "sp1"];
    if !known.contains(&backend.as_str()) {
        if let Some(candidate) = known.iter().find(|k| levenshtein(k, &backend) <= 2) {
            return Err(syn::Error::new(
                value.span(),
                format!(
                    "unknown backend `{backend}`; did you mean `{candidate}`? (custom backends are allowed — use `#[allow]`-free exact names and register an adapter implementing `ZkBackend`)"
                ),
            ));
        }
    }
    Ok(backend)
}

/// Plain Levenshtein distance, for the backend-typo suggestion.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0_usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        core::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
