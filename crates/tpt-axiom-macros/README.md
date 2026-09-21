# tpt-axiom-macros

Procedural macros for [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom),
currently the [`#[zk_provable]`] attribute macro.

`#[zk_provable]` analyzes an ordinary Rust function and generates:

1. the original function, unchanged — the "source of truth" Rust logic; and
2. a `CircuitDefinition` implementation that lowers the function's arithmetic
   constraints into a backend-agnostic `tpt_axiom_ir::ConstraintSystem`.

```rust,ignore
use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
}
```

## Supported syntax

- Parameters annotated with `#[public]` (default) or `#[secret]`, whose types
  are integer primitives.
- Straight-line `let` bindings built from `+`, `-`, `*`, parentheses, unary
  `-`, and integer literals.
- `assert!(a >= b)` / `assert!(a <= b)` / `assert!(a > b)` / `assert!(a < b)`
  / `assert!(a == b)` comparisons.
- `assert_eq!(l, r)`.
- An optional single integer `return`, which becomes a public output.

Everything else — control flow, dynamic allocation, function/method calls,
trait objects, closures — produces a compile-time error.

## Crate resolution

The macro resolves its supporting crates (`tpt-axiom-ir`, `tpt-axiom-zk`) at
expansion time via `proc-macro-crate`, falling back to the `tpt-axiom`
umbrella crate's re-exports when a caller depends on `tpt-axiom` rather than
the sub-crates directly. You shouldn't need to think about this in normal
use — it's why `use tpt_axiom::prelude::*;` alone is enough to use
`#[zk_provable]`.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
