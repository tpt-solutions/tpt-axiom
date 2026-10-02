//! The `#[zk_provable]` lowering: Rust AST → generated circuit definition.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    BinOp, Block, Error, Expr, ExprAssign, ExprBinary, ExprForLoop, ExprIf, ExprReturn, FnArg,
    ItemFn, Lit, LitStr, Local, Macro, Pat, ReturnType, Stmt, Type, UnOp,
};

/// Lowers an annotated function into (original fn + circuit definition).
pub fn lower_function(func: &ItemFn, backend: &str, register: bool) -> syn::Result<TokenStream> {
    validate_signature(func)?;

    let name = func.sig.ident.to_string();
    let struct_name = Ident::new(&pascal_case(&name), func.sig.ident.span());
    let vis = &func.vis;
    let has_return = !matches!(func.sig.output, ReturnType::Default);

    let ir_path = resolve_crate("tpt-axiom-ir", "tpt_axiom_ir");
    let zk_path = resolve_crate("tpt-axiom-zk", "tpt_axiom_zk");

    let output_int = match &func.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(int_type_of(ty, "return type")?),
    };

    let mut lowerer = Lowerer {
        has_return,
        tokens: TokenStream::new(),
        bound: BTreeSet::new(),
        types: BTreeMap::new(),
        counter: 0,
        output_bound: false,
        output_int,
        ir_path: ir_path.clone(),
        params: Vec::new(),
        gate: None,
        renames: BTreeMap::new(),
        mut_names: BTreeSet::new(),
    };
    lowerer.declare_params(func)?;
    lowerer.lower_block(&func.block)?;
    if has_return && !lowerer.output_bound {
        return Err(Error::new(
            func.sig.ident.span(),
            "this function must end with a `return <expr>;` or a trailing expression to become a public output",
        ));
    }
    let build_body = lowerer.tokens;
    let inputs = generate_inputs(
        &lowerer.params,
        has_return,
        &zk_path,
        vis,
        &struct_name,
        &name,
    );
    let register = if register {
        generate_register(&name, backend, &struct_name, &zk_path)
    } else {
        TokenStream::new()
    };

    let original = {
        let mut func = func.clone();
        for input in &mut func.sig.inputs {
            match input {
                FnArg::Receiver(r) => r.attrs.clear(),
                FnArg::Typed(pt) => pt.attrs.clear(),
            }
        }
        quote! {
            #[allow(dead_code)]
            #func
        }
    };

    Ok(quote! {
        #original

        /// Circuit definition generated from the annotated function.
        #[derive(Debug, Clone, Copy, Default)]
        #vis struct #struct_name;

        impl #zk_path::CircuitDefinition for #struct_name {
            fn name(&self) -> &'static str {
                #name
            }

            fn backend(&self) -> &'static str {
                #backend
            }

            fn build(&self) -> #ir_path::ConstraintSystem {
                let mut __axiom_builder = #ir_path::ConstraintSystemBuilder::new(#name);
                #build_body
                __axiom_builder.build()
            }
        }

        #register

        #inputs
    })
}

/// Generates the link-time registration for a circuit.
///
/// Emits a [`linkme`](https://docs.rs/linkme) distributed-slice constructor in
/// a private module, so `tpt_axiom_zk::registry::sorted()` enumerates every
/// circuit the linker kept. A circuit is only registered if something
/// references it, so dead-code elimination still works: an unused circuit costs
/// nothing and does not appear in `cargo axiom check`.
///
/// Emission is opt-in per function (`#[zk_provable(backend = "...", register)]`)
/// because it requires the `registry` feature of `tpt-axiom-zk`; a feature the
/// macro cannot detect in the *user's* crate. `linkme` is re-exported from
/// `tpt-axiom-zk`, so a `register` function needs no extra dependency of its
/// own.
fn generate_register(
    name: &str,
    backend: &str,
    struct_name: &Ident,
    zk_path: &TokenStream,
) -> TokenStream {
    let module = Ident::new(&format!("__axiom_register_{name}"), struct_name.span());
    quote! {
        #[doc(hidden)]
        mod #module {
            #[#zk_path::linkme::distributed_slice(#zk_path::REGISTERED_CIRCUITS)]
            #[linkme(crate = #zk_path::linkme)]
            static __AXIOM_CIRCUIT: #zk_path::RegisteredCircuit = #zk_path::RegisteredCircuit {
                name: #name,
                backend: #backend,
                build: || {
                    <super::#struct_name as #zk_path::CircuitDefinition>::build(
                        &super::#struct_name,
                    )
                },
            };
        }
    }
}

/// Generates the typed `Inputs` struct and its methods for an annotated
/// function.
///
/// The struct carries one field per parameter with the parameter's own Rust
/// type, so a caller cannot transpose two `u64` fields or feed a `u8` where a
/// `u64` is declared — the mistakes positional witness slices invite. Its
/// `named()` method produces a `NamedWitness`, which the zk driver resolves
/// and constraint-checks.
fn generate_inputs(
    params: &[ParamInfo],
    has_return: bool,
    zk_path: &TokenStream,
    vis: &syn::Visibility,
    struct_name: &Ident,
    circuit_name: &str,
) -> TokenStream {
    if params.is_empty() {
        return TokenStream::new();
    }
    // `prove_balance_transfer` -> `ProveBalanceTransferInputs`.
    let struct_ident = Ident::new(&format!("{struct_name}Inputs"), struct_name.span());

    let fields = params.iter().map(|p| {
        let ident = &p.ident;
        let ty = &p.ty;
        let doc = if p.secret {
            format!("Secret witness input `{ident}`.")
        } else {
            format!("Public input `{ident}`, visible to the verifier.")
        };
        quote! {
            #[doc = #doc]
            pub #ident: #ty
        }
    });

    let constructor_args = params.iter().map(|p| {
        let ident = &p.ident;
        let ty = &p.ty;
        quote!(#ident: #ty)
    });
    let constructor_fields = params.iter().map(|p| &p.ident);

    // Each field becomes one named assignment. Values are converted with
    // `try_from` where the declared type can exceed the IR's `i64` scalar
    // model, so an out-of-range value is a named error rather than a silent
    // `as` cast that would prove a different number.
    let assignments = params.iter().map(|p| {
        let ident = &p.ident;
        let name = ident.to_string();
        let conversion = if p.int_type.bits == 64 && !p.int_type.signed {
            quote! {
                ::core::convert::TryFrom::try_from(self.#ident).map_err(|_| {
                    #zk_path::NamedWitnessError::ScalarOutOfRange { name: #name }
                })?
            }
        } else {
            quote!(#zk_path::tpt_axiom_ir::Scalar::from(self.#ident))
        };
        quote! {
            named_witness.set(#name, #conversion);
        }
    });
    let output_doc: TokenStream = if has_return {
        quote! {
            /// The circuit's public output slot (`return`) is **not** filled in by
            /// `named()`: the output is computed by the circuit, not supplied by
            /// the prover. Use [`Self::with_output`] to bind it.
        }
    } else {
        quote! {}
    };
    let with_output = generate_with_output(has_return, zk_path);

    let public_names: Vec<String> = params
        .iter()
        .filter(|p| !p.secret)
        .map(|p| p.ident.to_string())
        .collect();
    let secret_names: Vec<String> = params
        .iter()
        .filter(|p| p.secret)
        .map(|p| p.ident.to_string())
        .collect();
    let public_slice = slice_of_strings(&public_names);
    let secret_slice = slice_of_strings(&secret_names);

    let circuit_doc = LitStr::new(circuit_name, struct_name.span());
    quote! {
        #[doc = concat!(
            "Typed inputs for the `", #circuit_doc, "` circuit, generated by ",
            "`#[zk_provable]`.\n\n",
            "One field per parameter, with the parameter's declared Rust type. ",
            "Fill it in, call `named()`, and hand the result to ",
            "`tpt_axiom_zk::prove_named`. Field order is irrelevant: values are ",
            "keyed by name, so a mis-ordered struct literal is not a silent ",
            "mis-statement."
        )]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #vis struct #struct_ident {
            #(#fields),*
        }

        impl #struct_ident {
            /// Builds the inputs from every field, in declaration order.
            #[must_use]
            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#constructor_args),*) -> Self {
                Self { #(#constructor_fields),* }
            }

            /// The circuit's public input names, in instance order.
            #[must_use]
            pub fn public_names() -> &'static [&'static str] {
                #public_slice
            }

            /// The circuit's secret input names, in witness order.
            #[must_use]
            pub fn secret_names() -> &'static [&'static str] {
                #secret_slice
            }

            #output_doc
            ///
            /// # Errors
            ///
            /// [`NamedWitnessError::ScalarOutOfRange`] when a value does not fit
            /// the IR's `i64` scalar model (an unsigned 64-bit input above
            /// `i64::MAX`).
            pub fn named(
                &self,
            ) -> ::core::result::Result<#zk_path::NamedWitness, #zk_path::NamedWitnessError> {
                let mut named_witness = #zk_path::NamedWitness::new();
                #(#assignments)*
                Ok(named_witness)
            }

            #with_output
        }
    }
}

/// The `with_output` helper generated for a circuit that returns a value.
fn generate_with_output(has_return: bool, zk_path: &TokenStream) -> TokenStream {
    if !has_return {
        return quote! {};
    }
    quote! {
        /// Binds the circuit's public output slot, which [`Self::named`] leaves unset.
        ///
        /// The output is a public input *of the proving instance*, but it is
        /// computed by the circuit rather than supplied by the prover — so it
        /// must be bound explicitly here rather than defaulted to zero.
        #[must_use]
        pub fn with_output(
            named_witness: #zk_path::NamedWitness,
            output: #zk_path::tpt_axiom_ir::Scalar,
        ) -> #zk_path::NamedWitness {
            let mut named_witness = named_witness;
            named_witness.set("return", output);
            named_witness
        }
    }
}

/// `&["a", "b"]` for a name list, or `&[]` when the list is empty.
fn slice_of_strings(names: &[String]) -> TokenStream {
    if names.is_empty() {
        return quote!(&[]);
    }
    let lits = names.iter().map(|n| {
        let lit = LitStr::new(n.as_str(), Span::call_site());
        quote!(#lit)
    });
    quote!(&[#(#lits),*])
}

/// Resolves the path to a supporting crate from the macro's point of view.
///
/// Priority: a direct dependency under its own name, then the `tpt-axiom`
/// umbrella crate (which re-exports both supporting crates), then the
/// conventional underscored crate name.
fn resolve_crate(package: &str, fallback: &str) -> TokenStream {
    use proc_macro_crate::{FoundCrate, crate_name};
    match crate_name(package) {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
        Err(_) => {
            if let Ok(FoundCrate::Name(umbrella)) = crate_name("tpt-axiom") {
                let umbrella = Ident::new(&umbrella, Span::call_site());
                let sub = Ident::new(fallback, Span::call_site());
                quote!(::#umbrella::#sub)
            } else {
                let ident = Ident::new(fallback, Span::call_site());
                quote!(::#ident)
            }
        }
    }
}

/// A declared integer width plus signedness, as recovered from Rust syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntSpec {
    bits: u32,
    signed: bool,
}

impl IntSpec {
    /// Signed 64-bit; the default the IR uses for hand-built systems.
    const I64: Self = Self {
        bits: 64,
        signed: true,
    };

    /// Unsigned 64-bit; the spec a `for` loop's constant counter carries.
    const U64: Self = Self {
        bits: 64,
        signed: false,
    };

    /// The IR constructor expression for this type, given the resolved path to
    /// `tpt-axiom-ir`.
    fn ir_expr(self, ir_path: &TokenStream) -> TokenStream {
        let variant = match (self.bits, self.signed) {
            (8, true) => "I8",
            (16, true) => "I16",
            (32, true) => "I32",
            (8, false) => "U8",
            (16, false) => "U16",
            (32, false) => "U32",
            (64, true) => "I64",
            _ => "U64",
        };
        let variant = Ident::new(variant, Span::call_site());
        quote!(#ir_path::IntType::#variant)
    }
}

/// Emits the [`tpt_axiom_ir::IntType`] expression for a declared type.
fn int_type_tokens(spec: IntSpec, ir_path: &TokenStream) -> TokenStream {
    spec.ir_expr(ir_path)
}

/// Recovers the integer spec from a Rust type, rejecting anything else.
fn int_type_of(ty: &Type, context: &str) -> syn::Result<IntSpec> {
    validate_integer_type(ty, context)?;
    let name = match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let spec = match name.as_str() {
        "u8" => IntSpec {
            bits: 8,
            signed: false,
        },
        "u16" => IntSpec {
            bits: 16,
            signed: false,
        },
        "u32" => IntSpec {
            bits: 32,
            signed: false,
        },
        "u64" | "usize" => IntSpec {
            bits: 64,
            signed: false,
        },
        "i8" => IntSpec {
            bits: 8,
            signed: true,
        },
        "i16" => IntSpec {
            bits: 16,
            signed: true,
        },
        "i32" => IntSpec {
            bits: 32,
            signed: true,
        },
        "i64" | "isize" => IntSpec {
            bits: 64,
            signed: true,
        },
        _ => {
            return Err(Error::new(
                ty.span(),
                format!(
                    "{context}: `{name}` is not a supported integer width (use u8/u16/u32/u64/usize or i8/i16/i32/i64/isize)"
                ),
            ));
        }
    };
    Ok(spec)
}

/// One annotated parameter, as the codegen needs to re-emit it.
struct ParamInfo {
    /// The Rust parameter's identifier.
    ident: Ident,
    /// The declared type, re-emitted verbatim on the generated `Inputs` struct.
    ty: Type,
    /// `true` for `#[secret]` parameters.
    secret: bool,
    /// The IR integer type, and whether it fits the IR's `i64` scalar model
    /// without a range check (signed 64-bit always does; unsigned 64-bit needs
    /// `try_from`).
    int_type: IntSpec,
}

struct Lowerer {
    has_return: bool,
    tokens: TokenStream,
    bound: BTreeSet<String>,
    /// The declared integer type of each bound name.
    types: BTreeMap<String, IntSpec>,
    counter: usize,
    output_bound: bool,
    /// The declared integer type of the public output (`None` when the function
    /// returns nothing).
    output_int: Option<IntSpec>,
    /// The resolved path to `tpt-axiom-ir`, used to emit `IntType` values.
    ir_path: TokenStream,
    /// The annotated parameters, in declaration order, for the generated
    /// `Inputs` struct.
    params: Vec<ParamInfo>,
    /// The enclosing conditional selector, as the generated Rust local holding
    /// its `ExprId` (`None` = unconditional). Inside an `if` branch every
    /// constraint is emitted as `gate·(·)` — a selector-gated product is
    /// vacuous exactly when the branch is not selected, which is how
    /// straight-line constraints encode branching soundly.
    gate: Option<Ident>,
    /// Source-name → generated-local renames, used for `for` loop counters:
    /// the counter binds a fresh generated local per iteration, so nothing
    /// after the loop can accidentally capture it.
    renames: BTreeMap<String, Ident>,
    /// Names declared `let mut` — the only targets `name = expr;` (and its
    /// `+=`/`-=`/`*=` forms) may rebind, as a fresh SSA value.
    mut_names: BTreeSet<String>,
}

/// The most iterations a `for i in a..b` loop may unroll.
///
/// Unrolling is the only way a fixed circuit can loop; a cap keeps a typo
/// (`0..1_000_000_000`) a fast compile error instead of a hang.
const MAX_LOOP_UNROLL: i64 = 1024;

impl Lowerer {
    fn fresh(&mut self) -> Ident {
        let id = Ident::new(&format!("__axiom_e{}", self.counter), Span::call_site());
        self.counter += 1;
        id
    }

    fn declare_params(&mut self, func: &ItemFn) -> syn::Result<()> {
        for input in &func.sig.inputs {
            let pt = match input {
                FnArg::Receiver(r) => {
                    return Err(Error::new(
                        r.span(),
                        "#[zk_provable] functions cannot take `self`; they must be free functions",
                    ));
                }
                FnArg::Typed(pt) => pt,
            };

            let mut secret = false;
            for attr in &pt.attrs {
                if attr.path().is_ident("secret") {
                    secret = true;
                } else if attr.path().is_ident("public") {
                    secret = false;
                } else {
                    return Err(Error::new(
                        attr.span(),
                        "unsupported parameter attribute; #[zk_provable] recognizes `#[public]` and `#[secret]`",
                    ));
                }
            }

            let ident = pattern_ident(&pt.pat)?;
            validate_integer_type(&pt.ty, "parameter type")?;

            let name = ident.to_string();
            let ctor = if secret {
                quote!(secret_input_typed)
            } else {
                quote!(public_input_typed)
            };
            let int_type = int_type_of(&pt.ty, "parameter type")?;
            let int_tokens = int_type_tokens(int_type, &self.ir_path);
            let binding = ident.clone();
            self.tokens.extend(quote! {
                let #binding = __axiom_builder.#ctor(#name, #int_tokens);
            });
            self.types.insert(name.clone(), int_type);
            self.bound.insert(name);
            self.params.push(ParamInfo {
                ident: ident.clone(),
                ty: (*pt.ty).clone(),
                secret,
                int_type,
            });
        }
        Ok(())
    }

    fn lower_block(&mut self, block: &Block) -> syn::Result<()> {
        let count = block.stmts.len();
        for (index, stmt) in block.stmts.iter().enumerate() {
            let is_last = index + 1 == count;
            self.lower_stmt(stmt, is_last)?;
        }
        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &Stmt, is_last: bool) -> syn::Result<()> {
        match stmt {
            Stmt::Local(local) => self.lower_local(local),
            Stmt::Macro(sm) => self.lower_macro(&sm.mac),
            // Bounded `for` loops and `if` statements lower through the
            // selector gadgets; everything else control-flow-shaped keeps its
            // dedicated error in `lower_expr_stmt`.
            Stmt::Expr(Expr::ForLoop(for_), _) => self.lower_for(for_),
            Stmt::Expr(Expr::If(if_), _) => self.lower_if_stmt(if_),
            Stmt::Expr(expr, semi) => {
                self.lower_expr_stmt(expr, semi.is_none() && is_last, is_last)
            }
            Stmt::Item(item) => Err(Error::new(
                item.span(),
                "nested items are not supported inside #[zk_provable] functions",
            )),
        }
    }

    /// Runs `f` with the current name/type scopes snapshotted, restoring them
    /// afterwards: bindings made inside a branch or loop body must not leak
    /// out (Rust block scoping).
    fn scoped<F>(&mut self, f: F) -> syn::Result<()>
    where
        F: FnOnce(&mut Self) -> syn::Result<()>,
    {
        let bound = self.bound.clone();
        let types = self.types.clone();
        let renames = self.renames.clone();
        let result = f(self);
        self.bound = bound;
        self.types = types;
        self.renames = renames;
        result
    }

    /// A free boolean selector: a circuit-internal witness the driver solves
    /// (never a named input). Uniquely numbered per circuit.
    fn emit_free_bool(&mut self) -> Ident {
        let t = self.fresh();
        let name = format!("__axiom_free{}", self.counter);
        self.counter += 1;
        let name = LitStr::new(&name, Span::call_site());
        self.tokens
            .extend(quote! { let #t = __axiom_builder.free_bool(#name); });
        t
    }

    /// `gate·expr` — the expression under the enclosing selector gate, or the
    /// expression itself when unconditional.
    fn gate_mul(&mut self, expr: &Ident) -> Ident {
        let gate = self.gate.clone();
        match gate {
            Some(g) => {
                let t = self.fresh();
                self.tokens
                    .extend(quote! { let #t = __axiom_builder.mul(#g, #expr); });
                t
            }
            None => expr.clone(),
        }
    }

    /// Force `s ∈ {0, 1}`: `s·s == s`, or the gated `g·(s·s − s) == 0` inside
    /// a branch. Deliberately *not* gated by `s`'s own branch condition — a
    /// two-valued selector stays two-valued whether or not its branch is
    /// active, which keeps the solver's candidate set exact.
    fn emit_booleanity(&mut self, s: &Ident) {
        let sq = self.fresh();
        self.tokens
            .extend(quote! { let #sq = __axiom_builder.mul(#s, #s); });
        let gate = self.gate.clone();
        match gate {
            Some(g) => {
                let diff = self.emit_sub(&sq, s);
                let gated = self.fresh();
                self.tokens
                    .extend(quote! { let #gated = __axiom_builder.mul(#g, #diff); });
                self.emit_zero(&gated);
            }
            None => self.emit_eq(&sq, s),
        }
    }

    /// The `a != b` gadget: a free selector `t` with `t·t == t`,
    /// `t·(a−b) − t ≥ 0` (so `t = 1 ⇒ a ≥ b + 1`) and
    /// `t·(a−b+1) − (a−b+1) ≥ 0` (so `t = 0 ⇒ a ≤ b − 1`). Satisfiable iff
    /// `a ≠ b` over the integers, and `t` is forced to the side the
    /// difference actually lies on — which is what makes the same gadget
    /// usable as the pinning half of an `!=` *condition*. `gate` is the
    /// fully-resolved enclosing selector (`None` = unconditional); when it is
    /// `Some`, every emitted constraint is vacuous unless the branch holding
    /// this gadget is selected.
    fn emit_ne_gadget(&mut self, l: &Ident, r: &Ident, gate: Option<&Ident>) -> Ident {
        let d = self.emit_sub(l, r);
        let t = self.emit_free_bool();
        // Booleanity, under the resolved gate (not `t`'s own branch gate).
        let sq = self.fresh();
        self.tokens
            .extend(quote! { let #sq = __axiom_builder.mul(#t, #t); });
        match gate {
            Some(g) => {
                let diff = self.emit_sub(&sq, &t);
                let gated = self.fresh();
                self.tokens
                    .extend(quote! { let #gated = __axiom_builder.mul(#g, #diff); });
                self.emit_zero(&gated);
            }
            None => self.emit_eq(&sq, &t),
        }
        // Arm 1: t = 1 ⇒ d ≥ 1.
        let td = self.fresh();
        self.tokens
            .extend(quote! { let #td = __axiom_builder.mul(#t, #d); });
        let arm1 = self.emit_sub(&td, &t);
        self.emit_gated_non_neg(&arm1, gate);
        // Arm 2: t = 0 ⇒ d ≤ −1, via s·(d+1) − (d+1) ≥ 0.
        let one = self.emit_const(1);
        let d1 = self.emit_add(&d, &one);
        let td1 = self.fresh();
        self.tokens
            .extend(quote! { let #td1 = __axiom_builder.mul(#t, #d1); });
        let arm2 = self.emit_sub(&td1, &d1);
        self.emit_gated_non_neg(&arm2, gate);
        t
    }

    /// `e ≥ 0`, multiplied by `gate` first when one is active.
    fn emit_gated_non_neg(&mut self, e: &Ident, gate: Option<&Ident>) {
        match gate {
            Some(g) => {
                let gated = self.fresh();
                self.tokens
                    .extend(quote! { let #gated = __axiom_builder.mul(#g, #e); });
                self.emit_nonneg(&gated);
            }
            None => self.emit_nonneg(e),
        }
    }

    /// Gated `e == 0`.
    fn emit_gated_zero(&mut self, e: &Ident, gate: Option<&Ident>) {
        match gate {
            Some(g) => {
                let gated = self.fresh();
                self.tokens
                    .extend(quote! { let #gated = __axiom_builder.mul(#g, #e); });
                self.emit_zero(&gated);
            }
            None => self.emit_zero(e),
        }
    }

    /// Pins the selector `s` (1 = condition true) to the truth of a
    /// comparison, under the enclosing gate.
    ///
    /// * `l ≥ r` (also `>`, `<`, `≤` after the ±1 adjustment): `s·d ≥ 0`
    ///   forces `s = 1 ⇒ d ≥ 0`, and `s·(d+1) − (d+1) ≥ 0` forces
    ///   `s = 0 ⇒ d ≤ −1`; together `s` is *forced* to the side the
    ///   difference lies on.
    /// * `l == r`: `s·d == 0` forces `s = 1 ⇒ d = 0`, and the `!=` gadget
    ///   under the gate `(1−s)` forces `s = 0 ⇒ d ≠ 0`.
    /// * `l != r`: the mirror image.
    fn lower_condition(&mut self, cond: &Expr, s: &Ident) -> syn::Result<()> {
        let bin = match cond {
            Expr::Paren(paren) => match &*paren.expr {
                Expr::Binary(bin) => bin,
                _ => {
                    return Err(Error::new(
                        cond.span(),
                        "`if` conditions must be comparisons such as `a >= b`",
                    ))
                }
            },
            Expr::Binary(bin) if is_comparison(bin.op) => bin,
            _ => {
                return Err(Error::new(
                    cond.span(),
                    "`if` conditions must be comparisons such as `a >= b`",
                ))
            }
        };
        let l = self.compile_expr(&bin.left)?;
        let r = self.compile_expr(&bin.right)?;
        let one = self.emit_const(1);
        let gate = self.gate.clone();
        match bin.op {
            BinOp::Ge(_) | BinOp::Gt(_) | BinOp::Le(_) | BinOp::Lt(_) => {
                // d is the signed gap that is ≥ 0 exactly when the condition
                // holds.
                let d = match bin.op {
                    BinOp::Ge(_) => self.emit_sub(&l, &r),
                    BinOp::Gt(_) => {
                        let raw = self.emit_sub(&l, &r);
                        self.emit_sub(&raw, &one)
                    }
                    BinOp::Le(_) => self.emit_sub(&r, &l),
                    _ => {
                        let raw = self.emit_sub(&r, &l);
                        self.emit_sub(&raw, &one)
                    }
                };
                // s = 1 ⇒ d ≥ 0.
                let sd = self.fresh();
                self.tokens
                    .extend(quote! { let #sd = __axiom_builder.mul(#s, #d); });
                self.emit_gated_non_neg(&sd, gate.as_ref());
                // s = 0 ⇒ d ≤ −1: s·(d+1) − (d+1) ≥ 0.
                let d1 = self.emit_add(&d, &one);
                let sd1 = self.fresh();
                self.tokens
                    .extend(quote! { let #sd1 = __axiom_builder.mul(#s, #d1); });
                let arm = self.emit_sub(&sd1, &d1);
                self.emit_gated_non_neg(&arm, gate.as_ref());
            }
            BinOp::Eq(_) => {
                let d = self.emit_sub(&l, &r);
                let sd = self.fresh();
                self.tokens
                    .extend(quote! { let #sd = __axiom_builder.mul(#s, #d); });
                self.emit_gated_zero(&sd, gate.as_ref());
                // s = 0 ⇒ d ≠ 0: the != gadget under the gate (1−s).
                let not_s = self.emit_sub(&one, s);
                let gate2 = self.gate_mul(&not_s);
                let _selector = self.emit_ne_gadget(&l, &r, Some(&gate2));
            }
            BinOp::Ne(_) => {
                let d = self.emit_sub(&l, &r);
                // s = 0 ⇒ d = 0.
                let not_s = self.emit_sub(&one, s);
                let nsd = self.fresh();
                self.tokens
                    .extend(quote! { let #nsd = __axiom_builder.mul(#not_s, #d); });
                self.emit_gated_zero(&nsd, gate.as_ref());
                // s = 1 ⇒ d ≠ 0: the != gadget under the gate s.
                let gate2 = self.gate_mul(s);
                let _selector = self.emit_ne_gadget(&l, &r, Some(&gate2));
            }
            _ => unreachable!("is_comparison filtered the operator"),
        }
        Ok(())
    }

    /// Lowers an `if` statement: a selector pinned to the condition's truth,
    /// then each branch's constraints re-stated under the branch's selector
    /// gate (`s` for the then-branch, `1−s` for the else-branch). Constraints
    /// in an unselected branch become identically-zero vacuities, and Rust
    /// block scoping is enforced by snapshotting the name scopes.
    fn lower_if_stmt(&mut self, expr_if: &ExprIf) -> syn::Result<()> {
        let s = self.emit_free_bool();
        self.emit_booleanity(&s);
        self.lower_condition(&expr_if.cond, &s)?;

        let then_gate = self.gate_mul(&s);
        self.scoped(|this| {
            let saved = this.gate.clone();
            this.gate = Some(then_gate);
            let result = this.lower_block(&expr_if.then_branch);
            this.gate = saved;
            result
        })?;
        if let Some((_, else_expr)) = &expr_if.else_branch {
            let else_block = match &**else_expr {
                Expr::Block(expr_block) => &expr_block.block,
                other => {
                    return Err(Error::new(
                        other.span(),
                        "the `else` branch of an `if` statement must be a block",
                    ))
                }
            };
            let one = self.emit_const(1);
            let not_s = self.emit_sub(&one, &s);
            let else_gate = self.gate_mul(&not_s);
            self.scoped(|this| {
                let saved = this.gate.clone();
                this.gate = Some(else_gate);
                let result = this.lower_block(else_block);
                this.gate = saved;
                result
            })?;
        }
        Ok(())
    }

    /// Lowers a bounded `for i in a..b` (or `a..=b`) loop by unrolling: the
    /// loop variable is a per-iteration constant, and the body lowers once
    /// per iteration in a fresh name scope. Bounds must be integer literals
    /// — a fixed circuit cannot depend on a runtime trip count.
    fn lower_for(&mut self, for_: &ExprForLoop) -> syn::Result<()> {
        // `for _ in ..` is the discard pattern; bind it under its own name,
        // which no expression can reference.
        let var = match &*for_.pat {
            Pat::Wild(_) => Ident::new("_", for_.pat.span()),
            pat => pattern_ident(pat)?,
        };
        let Expr::Range(range) = &*for_.expr else {
            return Err(Error::new(
                for_.expr.span(),
                "`for` loops must iterate over a literal integer range (`for i in a..b`)",
            ));
        };
        let start = match &range.start {
            None => 0,
            Some(e) => literal_value(e)?,
        };
        let end = literal_value(range.end.as_ref().ok_or_else(|| {
            Error::new(
                for_.expr.span(),
                "an unbounded range cannot lower to a fixed circuit; use a literal `a..b`",
            )
        })?)?;
        let count = if matches!(range.limits, syn::RangeLimits::Closed(_)) {
            end.checked_sub(start).and_then(|d| d.checked_add(1))
        } else {
            end.checked_sub(start)
        };
        let Some(count) = count else {
            return Err(Error::new(
                for_.expr.span(),
                "loop bounds overflow the IR's scalar model",
            ));
        };
        // A reversed range iterates zero times in Rust; unrolling zero
        // iterations is the faithful lowering.
        let count = count.max(0);
        if count > MAX_LOOP_UNROLL {
            return Err(Error::new(
                for_.expr.span(),
                format!(
                    "this loop unrolls to {count} iterations; #[zk_provable] caps unrolling at \
                     {MAX_LOOP_UNROLL} to keep circuits fixed-size and compile times sane"
                ),
            ));
        }
        let name = var.to_string();
        for iteration in 0..count {
            let value = start + iteration;
            // The counter is a per-iteration constant under a fresh generated
            // name, and the source name is renamed to it for exactly this
            // body. Nothing outside the loop can capture the counter, and
            // (unlike a Rust block) accumulator assignments inside the body
            // stay visible to the next iteration — `let mut acc` accumulates
            // the way the source reads.
            let counter = self.fresh();
            self.tokens.extend(quote! {
                let #counter = __axiom_builder.constant(#value);
            });
            let counter_handle = counter.clone();
            self.scoped(|this| {
                this.bound.insert(name.clone());
                this.types.insert(name.clone(), IntSpec::U64);
                this.renames.insert(name.clone(), counter_handle);
                this.lower_block(&for_.body)
            })?;
        }
        Ok(())
    }

    fn lower_local(&mut self, local: &Local) -> syn::Result<()> {
        if !local.attrs.is_empty() {
            return Err(Error::new(
                local.attrs[0].span(),
                "attributes on `let` bindings are not supported in #[zk_provable] functions",
            ));
        }
        let init = local.init.as_ref().ok_or_else(|| {
            Error::new(
                local.span(),
                "uninitialized `let` bindings are not supported; every binding must be a pure arithmetic expression",
            )
        })?;
        if init.diverge.is_some() {
            return Err(Error::new(
                init.expr.span(),
                "`let ... else` is not supported in #[zk_provable] functions",
            ));
        }
        let ident = pattern_ident(&local.pat)?;
        let handle = self.compile_expr(&init.expr)?;
        let name = ident.to_string();
        // `let mut` declares an SSA accumulator: assignments (`acc = …`,
        // `acc += …`) rebind it to a fresh value. Each rebind is a generated
        // shadow, so the DAG stays pure while the source reads naturally.
        if let Pat::Ident(pi) = &local.pat {
            if pi.mutability.is_some() {
                self.mut_names.insert(name.clone());
            }
        }
        if !self.bound.insert(name.clone()) {
            return Err(Error::new(
                ident.span(),
                format!(
                    "variable `{name}` is bound more than once; shadowing is not supported in #[zk_provable] functions"
                ),
            ));
        }
        // A `let x: T = ...` ascription is enforced, not ignored: when the
        // initialiser is a bare reference to an already-typed binding, the
        // ascription must agree with that binding's declared type.
        let declared = match &local.pat {
            Pat::Type(pt) => {
                let ascribed = int_type_of(&pt.ty, "`let` type ascription")?;
                if let Expr::Path(path) = &*init.expr {
                    if path.path.segments.len() == 1 {
                        let source = path.path.segments[0].ident.to_string();
                        if let Some(bound) = self.types.get(&source) {
                            if *bound != ascribed {
                                return Err(Error::new(
                                    pt.ty.span(),
                                    format!(
                                        "type ascription on `{name}` does not match the declared type of `{source}`"
                                    ),
                                ));
                            }
                        }
                    }
                }
                Some(ascribed)
            }
            _ => None,
        };
        self.types.insert(name, declared.unwrap_or(IntSpec::I64));
        self.tokens.extend(quote! {
            let #ident = #handle;
        });
        Ok(())
    }

    fn lower_expr_stmt(&mut self, expr: &Expr, is_tail: bool, is_last: bool) -> syn::Result<()> {
        match expr {
            Expr::Return(ret) => self.lower_return(ret, is_last),
            Expr::Macro(m) => self.lower_macro(&m.mac),
            Expr::Assign(assign) => self.lower_assignment(assign),
            Expr::Binary(bin) if is_compound_assign(bin.op) => self.lower_compound_assign(bin),
            Expr::If(_) | Expr::Match(_) | Expr::ForLoop(_) | Expr::While(_) | Expr::Loop(_) => {
                Err(Error::new(
                    expr.span(),
                    "control flow is not supported in #[zk_provable] functions; only straight-line arithmetic is allowed",
                ))
            }
            _ if is_tail => {
                if !self.has_return {
                    return Err(Error::new(
                        expr.span(),
                        "an expression statement is only supported as the function's return value",
                    ));
                }
                let handle = self.compile_expr(expr)?;
                self.bind_output(&handle, expr.span())
            }
            _ => Err(Error::new(
                expr.span(),
                "expression statements are not supported in #[zk_provable] functions",
            )),
        }
    }

    fn lower_return(&mut self, ret: &ExprReturn, is_last: bool) -> syn::Result<()> {
        let early = |span| {
            Error::new(
                span,
                "`return` must be the function's final statement; an early return is control flow, which #[zk_provable] cannot express as straight-line constraints",
            )
        };
        match (&ret.expr, self.has_return) {
            (Some(expr), true) => {
                if !is_last {
                    return Err(early(ret.span()));
                }
                let handle = self.compile_expr(expr)?;
                self.bind_output(&handle, expr.span())
            }
            (None, false) => {
                if !is_last {
                    return Err(early(ret.span()));
                }
                Ok(())
            }
            // A value-less `return` in a value-returning function, or the
            // reverse: the declared types alone reject it.
            (Some(_), false) => Err(Error::new(
                ret.span(),
                "this function has no return type, so it cannot return a value",
            )),
            (None, true) => Err(Error::new(
                ret.span(),
                "this function must return a value of its declared type",
            )),
        }
    }

    /// Lowers `name = expr;` / `name += expr;` / `-=` / `*=` for a `let mut`
    /// accumulator: the rebinding becomes a fresh SSA value (a generated
    /// shadow of the source name), so the source reads like mutation while
    /// the circuit stays a pure DAG.
    fn lower_assignment(&mut self, assign: &ExprAssign) -> syn::Result<()> {
        let Expr::Path(path) = &*assign.left else {
            return Err(Error::new(
                assign.left.span(),
                "only a simple named variable can be assigned in #[zk_provable] functions",
            ));
        };
        let ident = path.path.segments[0].ident.clone();
        let name = ident.to_string();
        if !self.mut_names.contains(&name) {
            return Err(Error::new(
                assign.span(),
                format!(
                    "`{name}` is not declared `let mut`; only `let mut` accumulators can be reassigned (each assignment becomes a fresh value in the circuit)"
                ),
            ));
        }
        let rhs = self.compile_expr(&assign.right)?;
        // A generated shadow: the source reads like mutation, the circuit
        // sees a fresh SSA value.
        self.tokens.extend(quote! { let #ident = #rhs; });
        Ok(())
    }

    /// Lowers `name += expr;` / `-=` / `*=` (syn parses compound assignments
    /// as binary expressions) onto a `let mut` accumulator.
    fn lower_compound_assign(&mut self, bin: &ExprBinary) -> syn::Result<()> {
        let Expr::Path(path) = &*bin.left else {
            return Err(Error::new(
                bin.left.span(),
                "only a simple named variable can be assigned in #[zk_provable] functions",
            ));
        };
        let ident = path.path.segments[0].ident.clone();
        let name = ident.to_string();
        if !self.mut_names.contains(&name) {
            return Err(Error::new(
                bin.span(),
                format!(
                    "`{name}` is not declared `let mut`; only `let mut` accumulators can be reassigned (each assignment becomes a fresh value in the circuit)"
                ),
            ));
        }
        let rhs = self.compile_expr(&bin.right)?;
        let method = match bin.op {
            BinOp::AddAssign(_) => "add",
            BinOp::SubAssign(_) => "sub",
            BinOp::MulAssign(_) => "mul",
            _ => {
                return Err(Error::new(
                    bin.span(),
                    "only `=`, `+=`, `-=` and `*=` are supported in #[zk_provable] functions",
                ));
            }
        };
        let method = Ident::new(method, Span::call_site());
        self.tokens
            .extend(quote! { let #ident = __axiom_builder.#method(#ident, #rhs); });
        Ok(())
    }

    fn bind_output(&mut self, handle: &Ident, span: Span) -> syn::Result<()> {
        if self.output_bound {
            return Err(Error::new(
                span,
                "#[zk_provable] functions may return exactly once",
            ));
        }
        let out = self.fresh();
        let spec = self.output_int.unwrap_or(IntSpec::I64);
        let int_tokens = int_type_tokens(spec, &self.ir_path);
        self.tokens.extend(quote! {
            let #out = __axiom_builder.output_typed("return", #int_tokens);
            __axiom_builder.constrain_eq(#out, #handle);
        });
        self.output_bound = true;
        Ok(())
    }

    fn lower_macro(&mut self, mac: &Macro) -> syn::Result<()> {
        let segment = mac
            .path
            .segments
            .last()
            .ok_or_else(|| Error::new(mac.span(), "empty macro path"))?;
        match segment.ident.to_string().as_str() {
            "assert" | "debug_assert" => {
                let parsed: OneExpr = syn::parse2(mac.tokens.clone()).map_err(|_| {
                    Error::new(
                        mac.span(),
                        "`assert!` in #[zk_provable] takes exactly one boolean expression; format messages are not supported",
                    )
                })?;
                self.lower_assertion(&parsed.0)
            }
            "assert_eq" | "debug_assert_eq" => {
                let parsed: TwoExprs = syn::parse2(mac.tokens.clone()).map_err(|_| {
                    Error::new(
                        mac.span(),
                        "`assert_eq!` in #[zk_provable] takes exactly two arguments",
                    )
                })?;
                let l = self.compile_expr(&parsed.0)?;
                let r = self.compile_expr(&parsed.1)?;
                self.emit_eq(&l, &r);
                Ok(())
            }
            other => Err(Error::new(
                segment.ident.span(),
                format!(
                    "macro `{other}!` is not supported in #[zk_provable] functions (only `assert!`, `assert_eq!`, `debug_assert!` and `debug_assert_eq!` are recognized)"
                ),
            )),
        }
    }

    /// Lowers one boolean claim from `assert!`: either a comparison or a
    /// conjunction (`&&`) of comparisons. `debug_assert!` lands here too — a
    /// circuit has no debug builds, so the claim is always enforced.
    fn lower_assertion(&mut self, expr: &Expr) -> syn::Result<()> {
        match expr {
            Expr::Binary(bin) if matches!(bin.op, BinOp::And(_)) => {
                self.lower_assertion(&bin.left)?;
                self.lower_assertion(&bin.right)?;
                Ok(())
            }
            Expr::Binary(bin) => self.lower_comparison(bin),
            other => Err(Error::new(
                other.span(),
                "`assert!` in #[zk_provable] must contain a comparison such as `a >= b` (conjunctions with `&&` are allowed)",
            )),
        }
    }

    fn lower_comparison(&mut self, bin: &ExprBinary) -> syn::Result<()> {
        let l = self.compile_expr(&bin.left)?;
        let r = self.compile_expr(&bin.right)?;
        match bin.op {
            BinOp::Ge(_) => {
                let d = self.emit_sub(&l, &r);
                self.emit_nonneg(&d);
            }
            BinOp::Le(_) => {
                let d = self.emit_sub(&r, &l);
                self.emit_nonneg(&d);
            }
            BinOp::Gt(_) => {
                let d = self.emit_sub(&l, &r);
                let one = self.emit_const(1);
                let e = self.emit_sub(&d, &one);
                self.emit_nonneg(&e);
            }
            BinOp::Lt(_) => {
                let d = self.emit_sub(&r, &l);
                let one = self.emit_const(1);
                let e = self.emit_sub(&d, &one);
                self.emit_nonneg(&e);
            }
            BinOp::Eq(_) => self.emit_eq(&l, &r),
            BinOp::Ne(_) => {
                // The inequality gadget: a free selector bit forced onto one
                // side of the difference, so a satisfying assignment exists
                // iff the operands really differ. Sound over the integers:
                // both arms are range checks, and `l == r` satisfies neither.
                let gate = self.gate.clone();
                let _selector = self.emit_ne_gadget(&l, &r, gate.as_ref());
            }
            BinOp::Or(_) => {
                return Err(Error::new(
                    bin.span(),
                    "`||` cannot be expressed as arithmetic constraints; use separate `assert!`s or a conjunction with `&&`",
                ));
            }
            _ => {
                return Err(Error::new(
                    bin.span(),
                    "the comparison in `assert!` must be one of `>`, `>=`, `<`, `<=` or `==`",
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // one arm per unsupported Rust construct
    fn compile_expr(&mut self, expr: &Expr) -> syn::Result<Ident> {
        match expr {
            Expr::Path(path) => {
                if path.qself.is_some()
                    || path.path.leading_colon.is_some()
                    || path.path.segments.len() != 1
                {
                    return Err(Error::new(
                        path.span(),
                        "only simple local variables and parameters may be referenced in #[zk_provable] functions",
                    ));
                }
                let ident = path.path.segments[0].ident.clone();
                if !self.bound.contains(&ident.to_string()) {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "cannot resolve `{ident}` in #[zk_provable]: only parameters and earlier `let` bindings are visible"
                        ),
                    ));
                }
                // A `for` counter maps to its per-iteration generated local.
                Ok(self
                    .renames
                    .get(&ident.to_string())
                    .cloned()
                    .unwrap_or(ident))
            }
            Expr::Lit(lit) => match &lit.lit {
                Lit::Int(int) => {
                    let value: i64 = int.base10_parse().map_err(|_| {
                        Error::new(
                            int.span(),
                            "integer literal does not fit the IR's scalar model (i64)",
                        )
                    })?;
                    Ok(self.emit_const(value))
                }
                other => Err(Error::new(
                    other.span(),
                    "only integer literals are supported in #[zk_provable] arithmetic",
                )),
            },
            Expr::Paren(paren) => self.compile_expr(&paren.expr),
            Expr::Unary(unary) => match unary.op {
                UnOp::Neg(_) => {
                    let inner = self.compile_expr(&unary.expr)?;
                    let t = self.fresh();
                    self.tokens
                        .extend(quote! { let #t = __axiom_builder.neg(#inner); });
                    Ok(t)
                }
                _ => Err(Error::new(
                    unary.span(),
                    "only unary negation is supported in #[zk_provable] arithmetic",
                )),
            },
            Expr::Binary(bin) => {
                let l = self.compile_expr(&bin.left)?;
                let r = self.compile_expr(&bin.right)?;
                let t = self.fresh();
                match bin.op {
                    BinOp::Add(_) | BinOp::Sub(_) | BinOp::Mul(_) => {
                        let method = match bin.op {
                            BinOp::Add(_) => "add",
                            BinOp::Sub(_) => "sub",
                            _ => "mul",
                        };
                        let method = Ident::new(method, Span::call_site());
                        self.tokens
                            .extend(quote! { let #t = __axiom_builder.#method(#l, #r); });
                    }
                    BinOp::Div(_) | BinOp::Rem(_) => {
                        // The quotient/remainder gadget. Sound only for
                        // unsigned operands: truncated division of a
                        // negative dividend leaves a non-positive
                        // remainder, which the gadget's range checks would
                        // reject at prove time — so signed division is a
                        // compile error with that explanation.
                        let (l_spec, r_spec) = (
                            self.spec_of(&bin.left, &l)?,
                            self.spec_of(&bin.right, &r)?,
                        );
                        if l_spec.signed || r_spec.signed {
                            return Err(Error::new(
                                bin.span(),
                                "division is supported for unsigned operands only: the quotient/remainder gadget constrains a non-negative remainder, which truncated division of a negative value would violate. Express signed division through explicit multiplication instead",
                            ));
                        }
                        let is_rem = matches!(bin.op, BinOp::Rem(_));
                        self.tokens.extend(quote! {
                            let __axiom_quotient = __axiom_builder.div_trunc(#l, #r);
                            __axiom_builder.constrain_division(#l, #r, __axiom_quotient);
                        });
                        if is_rem {
                            // remainder = dividend - quotient * divisor.
                            let product = self.fresh();
                            self.tokens.extend(quote! {
                                let #product = __axiom_builder.mul(__axiom_quotient, #r);
                                let #t = __axiom_builder.sub(#l, #product);
                            });
                        } else {
                            self.tokens
                                .extend(quote! { let #t = __axiom_quotient; });
                        }
                    }
                    _ => {
                        return Err(Error::new(
                            bin.span(),
                            "only `+`, `-`, `*`, `/` and `%` are supported in #[zk_provable] arithmetic",
                        ));
                    }
                }
                Ok(t)
            }
            Expr::Call(call) => Err(Error::new(
                call.span(),
                "function calls are not supported in #[zk_provable] functions (this includes heap allocation such as `String::new()` or `vec![]`)",
            )),
            Expr::MethodCall(call) => Err(Error::new(
                call.span(),
                "method calls are not supported in #[zk_provable] functions",
            )),
            Expr::Field(field) => Err(Error::new(
                field.span(),
                "field access is not supported in #[zk_provable] functions",
            )),
            Expr::Index(index) => Err(Error::new(
                index.span(),
                "indexing is not supported in #[zk_provable] functions (there is no heap data on the circuit)",
            )),
            Expr::Closure(closure) => Err(Error::new(
                closure.span(),
                "closures are not supported in #[zk_provable] functions",
            )),
            Expr::Assign(assign) => Err(Error::new(
                assign.span(),
                "mutation is not supported in #[zk_provable] functions; bind a new value with `let` instead",
            )),
            Expr::Reference(reference) => Err(Error::new(
                reference.span(),
                "references are not supported in #[zk_provable] functions",
            )),
            Expr::Cast(cast) => Err(Error::new(
                cast.span(),
                "casts are not supported in #[zk_provable] functions; use matching integer types",
            )),
            Expr::If(if_expr) => {
                // `let x = if c { a } else { b };` — a multiplexed value:
                // `b + s·(a − b)` with `s` a selector pinned to the truth of
                // `c`. Both branch values are computed unconditionally (they
                // are pure arithmetic); only the *selection* is conditional.
                let Some((_, else_expr)) = &if_expr.else_branch else {
                    return Err(Error::new(
                        if_expr.span(),
                        "an `if` expression needs an `else` branch; a circuit computes both arms and selects, so there is no fall-through",
                    ));
                };
                let Some(a) = block_tail_expr(&if_expr.then_branch) else {
                    return Err(Error::new(
                        if_expr.then_branch.span(),
                        "an `if` expression's branches must be single arithmetic expressions",
                    ));
                };
                let b = match &**else_expr {
                    Expr::Block(expr_block) => {
                        block_tail_expr(&expr_block.block).ok_or_else(|| {
                            Error::new(
                                else_expr.span(),
                                "an `if` expression's `else` branch must be a single arithmetic expression",
                            )
                        })?
                    }
                    other => other,
                };
                let s = self.emit_free_bool();
                self.emit_booleanity(&s);
                self.lower_condition(&if_expr.cond, &s)?;
                let a_handle = self.compile_expr(a)?;
                let b_handle = self.compile_expr(b)?;
                let diff = self.emit_sub(&a_handle, &b_handle);
                let selected = self.fresh();
                self.tokens
                    .extend(quote! { let #selected = __axiom_builder.mul(#s, #diff); });
                let t = self.emit_add(&b_handle, &selected);
                Ok(t)
            }
            Expr::ForLoop(_) | Expr::While(_) | Expr::Loop(_) => Err(Error::new(
                expr.span(),
                "loops are not supported in #[zk_provable] functions (dynamic bounds cannot be lowered to a fixed circuit)",
            )),
            Expr::Return(_) => Err(Error::new(
                expr.span(),
                "`return` may not be used as a sub-expression in #[zk_provable] functions",
            )),
            other => Err(Error::new(
                other.span(),
                "unsupported expression in #[zk_provable] function; only integer arithmetic over `+`, `-`, `*` and literals is allowed",
            )),
        }
    }

    /// The integer spec an operand carries: the declared type of a name
    /// binding, unsigned-64 for literals, or the wider of the two operands
    /// for an intermediate (its source expression's own specs).
    fn spec_of(&self, expr: &Expr, handle: &Ident) -> syn::Result<IntSpec> {
        let _ = handle;
        match expr {
            Expr::Path(path) => {
                let name = path.path.segments[0].ident.to_string();
                Ok(self.types.get(&name).copied().unwrap_or(IntSpec::I64))
            }
            Expr::Lit(lit) => {
                let _ = lit;
                Ok(IntSpec {
                    bits: 64,
                    signed: false,
                })
            }
            Expr::Binary(bin) => {
                // The handle was produced from these operands; their specs
                // join to the result's.
                let l = self.spec_of(&bin.left, handle)?;
                let r = self.spec_of(&bin.right, handle)?;
                Ok(IntSpec {
                    bits: l.bits.max(r.bits),
                    signed: l.signed || r.signed,
                })
            }
            Expr::Paren(paren) => self.spec_of(&paren.expr, handle),
            Expr::Unary(unary) => self.spec_of(&unary.expr, handle),
            _ => Ok(IntSpec::I64),
        }
    }

    fn emit_const(&mut self, value: i64) -> Ident {
        let t = self.fresh();
        self.tokens
            .extend(quote! { let #t = __axiom_builder.constant(#value); });
        t
    }

    fn emit_sub(&mut self, l: &Ident, r: &Ident) -> Ident {
        let t = self.fresh();
        self.tokens
            .extend(quote! { let #t = __axiom_builder.sub(#l, #r); });
        t
    }

    fn emit_add(&mut self, l: &Ident, r: &Ident) -> Ident {
        let t = self.fresh();
        self.tokens
            .extend(quote! { let #t = __axiom_builder.add(#l, #r); });
        t
    }

    fn emit_zero(&mut self, e: &Ident) {
        self.tokens
            .extend(quote! { __axiom_builder.constrain_zero(#e); });
    }

    fn emit_eq(&mut self, l: &Ident, r: &Ident) {
        self.tokens
            .extend(quote! { __axiom_builder.constrain_eq(#l, #r); });
    }

    fn emit_nonneg(&mut self, e: &Ident) {
        self.tokens
            .extend(quote! { __axiom_builder.constrain_non_negative(#e); });
    }
}

/// Parses a single expression and rejects trailing tokens (e.g. format messages).
struct OneExpr(Expr);

impl Parse for OneExpr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let expr: Expr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected extra tokens"));
        }
        Ok(Self(expr))
    }
}

/// Parses exactly two comma-separated expressions.
struct TwoExprs(Expr, Expr);

impl Parse for TwoExprs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let a: Expr = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let b: Expr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected extra tokens"));
        }
        Ok(Self(a, b))
    }
}

fn pattern_ident(pat: &Pat) -> syn::Result<Ident> {
    match pat {
        Pat::Ident(pi) if pi.by_ref.is_none() && pi.subpat.is_none() => Ok(pi.ident.clone()),
        Pat::Type(pt) => pattern_ident(&pt.pat),
        _ => Err(Error::new(
            pat.span(),
            "unsupported pattern; #[zk_provable] functions only bind simple named variables",
        )),
    }
}

/// Whether a binary operator is one of the compound assignments (`+=`, `-=`,
/// `*=`) an accumulator may use.
fn is_compound_assign(op: BinOp) -> bool {
    matches!(op, BinOp::AddAssign(_) | BinOp::SubAssign(_) | BinOp::MulAssign(_))
}

/// Whether a binary operator is one of the six comparisons an `if` condition
/// may use.
fn is_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Ge(_)
            | BinOp::Gt(_)
            | BinOp::Le(_)
            | BinOp::Lt(_)
            | BinOp::Eq(_)
            | BinOp::Ne(_)
    )
}

/// The integer value of a (possibly negated) integer literal, for loop bounds.
fn literal_value(expr: &Expr) -> syn::Result<i64> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => int.base10_parse::<i64>().map_err(|_| {
                Error::new(
                    int.span(),
                    "integer literal does not fit the IR's scalar model (i64)",
                )
            }),
            other => Err(Error::new(
                other.span(),
                "only integer literals may bound a `for` loop",
            )),
        },
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
            let inner = literal_value(&unary.expr)?;
            inner.checked_neg().ok_or_else(|| {
                Error::new(unary.span(), "integer literal does not fit the IR's scalar model")
            })
        }
        Expr::Paren(paren) => literal_value(&paren.expr),
        _ => Err(Error::new(
            expr.span(),
            "`for` bounds must be integer literals; a fixed circuit cannot depend on a runtime trip count",
        )),
    }
}

/// The single tail expression of a block (`{ expr }`), if that is all it is.
fn block_tail_expr(block: &Block) -> Option<&Expr> {
    match block.stmts.as_slice() {
        [Stmt::Expr(expr, None)] => Some(expr),
        _ => None,
    }
}

fn validate_integer_type(ty: &Type, context: &str) -> syn::Result<()> {
    if let Type::Path(tp) = ty {
        if tp.qself.is_none() {
            if let Some(segment) = tp.path.segments.last() {
                let name = segment.ident.to_string();
                if is_integer_primitive(&name) {
                    return Ok(());
                }
                return Err(Error::new(
                    ty.span(),
                    format!(
                        "{context}: `{name}` is not an integer primitive; #[zk_provable] supports only integer primitives (no heap types, trait objects, or generics)"
                    ),
                ));
            }
        }
    }
    Err(Error::new(
        ty.span(),
        format!(
            "{context}: unsupported type; #[zk_provable] supports only integer primitives (no heap types, trait objects, or generics)"
        ),
    ))
}

fn is_integer_primitive(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
    )
}

fn validate_signature(func: &ItemFn) -> syn::Result<()> {
    let sig = &func.sig;
    if let Some(asyncness) = &sig.asyncness {
        return Err(Error::new(
            asyncness.span(),
            "`async` functions cannot be circuits",
        ));
    }
    if let Some(unsafety) = &sig.unsafety {
        return Err(Error::new(
            unsafety.span(),
            "`unsafe` functions cannot be circuits",
        ));
    }
    if let Some(abi) = &sig.abi {
        return Err(Error::new(
            abi.span(),
            "`extern` functions cannot be circuits",
        ));
    }
    if let Some(constness) = &sig.constness {
        return Err(Error::new(
            constness.span(),
            "`const fn` cannot be a circuit",
        ));
    }
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(Error::new(
            sig.generics.span(),
            "generic #[zk_provable] functions are not supported; the circuit must be monomorphic",
        ));
    }
    if let Some(variadic) = &sig.variadic {
        return Err(Error::new(
            variadic.span(),
            "variadic functions cannot be circuits",
        ));
    }
    if let ReturnType::Type(_, ty) = &sig.output {
        validate_integer_type(ty, "return type")?;
    }
    Ok(())
}

fn pascal_case(name: &str) -> String {
    let mut out = String::new();
    for part in name.split('_').filter(|part| !part.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        out.push_str("ZkCircuit");
    }
    out
}
