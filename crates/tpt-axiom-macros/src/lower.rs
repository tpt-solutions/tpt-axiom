//! The `#[zk_provable]` lowering: Rust AST → generated circuit definition.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    BinOp, Block, Error, Expr, ExprBinary, ExprReturn, FnArg, ItemFn, Lit, LitStr, Local, Macro,
    Pat, ReturnType, Stmt, Type, UnOp,
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
}

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
            Stmt::Expr(expr, semi) => {
                self.lower_expr_stmt(expr, semi.is_none() && is_last, is_last)
            }
            Stmt::Item(item) => Err(Error::new(
                item.span(),
                "nested items are not supported inside #[zk_provable] functions",
            )),
        }
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
        if let Pat::Ident(pi) = &local.pat {
            if pi.mutability.is_some() {
                return Err(Error::new(
                    pi.mutability.span(),
                    "`mut` bindings are not supported; #[zk_provable] functions are a pure expression DAG",
                ));
            }
        }
        let handle = self.compile_expr(&init.expr)?;
        let name = ident.to_string();
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
                return Err(Error::new(
                    bin.span(),
                    "`!=` cannot be expressed as an arithmetic constraint; use a range check or equality instead",
                ));
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
                Ok(ident)
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
                let method = match bin.op {
                    BinOp::Add(_) => "add",
                    BinOp::Sub(_) => "sub",
                    BinOp::Mul(_) => "mul",
                    BinOp::Div(_) | BinOp::Rem(_) => {
                        return Err(Error::new(
                            bin.span(),
                            "division and remainder are not supported as circuit arithmetic; express them as multiplication by a public/secret inverse explicitly",
                        ));
                    }
                    _ => {
                        return Err(Error::new(
                            bin.span(),
                            "only `+`, `-` and `*` are supported in #[zk_provable] arithmetic",
                        ));
                    }
                };
                let l = self.compile_expr(&bin.left)?;
                let r = self.compile_expr(&bin.right)?;
                let method = Ident::new(method, Span::call_site());
                let t = self.fresh();
                self.tokens
                    .extend(quote! { let #t = __axiom_builder.#method(#l, #r); });
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
            Expr::If(_) | Expr::Match(_) => Err(Error::new(
                expr.span(),
                "conditional logic is not supported in #[zk_provable] functions; it cannot be represented as straight-line arithmetic constraints",
            )),
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
