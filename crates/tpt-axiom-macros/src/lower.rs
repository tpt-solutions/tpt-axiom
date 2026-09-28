//! The `#[zk_provable]` lowering: Rust AST → generated circuit definition.

use std::collections::BTreeSet;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    BinOp, Block, Error, Expr, ExprBinary, ExprReturn, FnArg, ItemFn, Lit, Local, Macro, Pat,
    ReturnType, Stmt, Type, UnOp,
};

/// Lowers an annotated function into (original fn + circuit definition).
pub fn lower_function(func: &ItemFn, backend: &str) -> syn::Result<TokenStream> {
    validate_signature(func)?;

    let name = func.sig.ident.to_string();
    let struct_name = Ident::new(&pascal_case(&name), func.sig.ident.span());
    let vis = &func.vis;
    let has_return = !matches!(func.sig.output, ReturnType::Default);

    let ir_path = resolve_crate("tpt-axiom-ir", "tpt_axiom_ir");
    let zk_path = resolve_crate("tpt-axiom-zk", "tpt_axiom_zk");

    let mut lowerer = Lowerer {
        has_return,
        tokens: TokenStream::new(),
        bound: BTreeSet::new(),
        counter: 0,
        output_bound: false,
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
    })
}

/// Resolves the path to a supporting crate from the macro's point of view.
///
/// Priority: a direct dependency under its own name, then the `tpt-axiom`
/// umbrella crate (which re-exports both supporting crates), then the
/// conventional underscored crate name.
fn resolve_crate(package: &str, fallback: &str) -> TokenStream {
    use proc_macro_crate::{crate_name, FoundCrate};
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

struct Lowerer {
    has_return: bool,
    tokens: TokenStream,
    bound: BTreeSet<String>,
    counter: usize,
    output_bound: bool,
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
                    ))
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
                quote!(secret_input)
            } else {
                quote!(public_input)
            };
            let binding = ident.clone();
            self.tokens.extend(quote! {
                let #binding = __axiom_builder.#ctor(#name);
            });
            self.bound.insert(name);
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
            Stmt::Expr(expr, semi) => self.lower_expr_stmt(expr, semi.is_none() && is_last),
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
                format!("variable `{name}` is bound more than once; shadowing is not supported in #[zk_provable] functions"),
            ));
        }
        self.tokens.extend(quote! {
            let #ident = #handle;
        });
        Ok(())
    }

    fn lower_expr_stmt(&mut self, expr: &Expr, is_tail: bool) -> syn::Result<()> {
        match expr {
            Expr::Return(ret) => self.lower_return(ret),
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

    fn lower_return(&mut self, ret: &ExprReturn) -> syn::Result<()> {
        match &ret.expr {
            Some(expr) => {
                if !self.has_return {
                    return Err(Error::new(
                        ret.span(),
                        "this function has no return type, so it cannot return a value",
                    ));
                }
                let handle = self.compile_expr(expr)?;
                self.bind_output(&handle, expr.span())
            }
            None => {
                if self.has_return {
                    Err(Error::new(
                        ret.span(),
                        "this function must return a value of its declared type",
                    ))
                } else {
                    Ok(())
                }
            }
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
        self.tokens.extend(quote! {
            let #out = __axiom_builder.output("return");
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
            "assert" => {
                let parsed: OneExpr = syn::parse2(mac.tokens.clone()).map_err(|_| {
                    Error::new(
                        mac.span(),
                        "`assert!` in #[zk_provable] takes exactly one comparison; format messages are not supported",
                    )
                })?;
                let bin = match &parsed.0 {
                    Expr::Binary(bin) => bin,
                    other => {
                        return Err(Error::new(
                            other.span(),
                            "`assert!` in #[zk_provable] must contain a comparison such as `a >= b`",
                        ))
                    }
                };
                self.lower_comparison(bin)
            }
            "assert_eq" => {
                let parsed: TwoExprs = syn::parse2(mac.tokens.clone()).map_err(|_| {
                    Error::new(
                        mac.span(),
                        "`assert_eq!` in #[zk_provable] takes exactly two arguments",
                    )
                })?;
                let l = self.compile_expr(&parsed.0)?;
                let r = self.compile_expr(&parsed.1)?;
                self.emit_eq(l, r);
                Ok(())
            }
            other => Err(Error::new(
                segment.ident.span(),
                format!("macro `{other}!` is not supported in #[zk_provable] functions (only `assert!` and `assert_eq!` are recognized)"),
            )),
        }
    }

    fn lower_comparison(&mut self, bin: &ExprBinary) -> syn::Result<()> {
        let l = self.compile_expr(&bin.left)?;
        let r = self.compile_expr(&bin.right)?;
        match bin.op {
            BinOp::Ge(_) => {
                let d = self.emit_sub(l, r);
                self.emit_nonneg(d);
            }
            BinOp::Le(_) => {
                let d = self.emit_sub(r, l);
                self.emit_nonneg(d);
            }
            BinOp::Gt(_) => {
                let d = self.emit_sub(l, r);
                let one = self.emit_const(1);
                let e = self.emit_sub(d, one);
                self.emit_nonneg(e);
            }
            BinOp::Lt(_) => {
                let d = self.emit_sub(r, l);
                let one = self.emit_const(1);
                let e = self.emit_sub(d, one);
                self.emit_nonneg(e);
            }
            BinOp::Eq(_) => self.emit_eq(l, r),
            BinOp::Ne(_) => {
                return Err(Error::new(
                    bin.span(),
                    "`!=` cannot be expressed as an arithmetic constraint; use a range check or equality instead",
                ))
            }
            _ => {
                return Err(Error::new(
                    bin.span(),
                    "the comparison in `assert!` must be one of `>`, `>=`, `<`, `<=` or `==`",
                ))
            }
        }
        Ok(())
    }

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
                        format!("cannot resolve `{ident}` in #[zk_provable]: only parameters and earlier `let` bindings are visible"),
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
                        ))
                    }
                    _ => {
                        return Err(Error::new(
                            bin.span(),
                            "only `+`, `-` and `*` are supported in #[zk_provable] arithmetic",
                        ))
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

    fn emit_sub(&mut self, l: Ident, r: Ident) -> Ident {
        let t = self.fresh();
        self.tokens
            .extend(quote! { let #t = __axiom_builder.sub(#l, #r); });
        t
    }

    fn emit_eq(&mut self, l: Ident, r: Ident) {
        self.tokens
            .extend(quote! { __axiom_builder.constrain_eq(#l, #r); });
    }

    fn emit_nonneg(&mut self, e: Ident) {
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
        Ok(OneExpr(expr))
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
        Ok(TwoExprs(a, b))
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
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
    )
}

fn validate_signature(func: &ItemFn) -> syn::Result<()> {
    let sig = &func.sig;
    if let Some(asyncness) = &sig.asyncness {
        return Err(Error::new(asyncness.span(), "`async` functions cannot be circuits"));
    }
    if let Some(unsafety) = &sig.unsafety {
        return Err(Error::new(unsafety.span(), "`unsafe` functions cannot be circuits"));
    }
    if let Some(abi) = &sig.abi {
        return Err(Error::new(abi.span(), "`extern` functions cannot be circuits"));
    }
    if let Some(constness) = &sig.constness {
        return Err(Error::new(constness.span(), "`const fn` cannot be a circuit"));
    }
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(Error::new(
            sig.generics.span(),
            "generic #[zk_provable] functions are not supported; the circuit must be monomorphic",
        ));
    }
    if let Some(variadic) = &sig.variadic {
        return Err(Error::new(variadic.span(), "variadic functions cannot be circuits"));
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