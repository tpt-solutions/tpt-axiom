# Zero-knowledge circuits

Annotate a plain function; the macro keeps it runnable as Rust *and* lowers
it to a backend-agnostic IR:

```rust
#[zk_provable(backend = "halo2")]
fn prove_age_over(
    #[public] current_year: u32,
    #[public] min_age: u32,
    #[secret] birth_year: u32,
) {
    assert!(current_year - birth_year >= min_age);
}
```

Supported syntax: straight-line integer arithmetic (`+`, `-`, `*`, and
`/`/`%` on unsigned operands via the quotient/remainder gadget), `let`
bindings and `let mut` accumulators (`acc += …` rebinding — each assignment
becomes a fresh value in the circuit), `assert!`/`assert_eq!` (conjunctions
with `&&`), `debug_assert!` (a circuit has no debug builds — it is always
enforced), `!=` (see below), `if` statements and `if`/`else` expressions,
and bounded `for i in a..b` loops (literal bounds, unrolled).

Everything else is a clear compile error, including early returns, signed
division, and loops with runtime bounds.

## Inequality: `!=`

`assert!(a != b)` lowers to an *inequality gadget*: a free boolean selector
`t` with `t·t == t` (booleanity), `t·(a−b) − t ≥ 0` (so `t = 1 ⇒ a ≥ b + 1`)
and `t·(a−b+1) − (a−b+1) ≥ 0` (so `t = 0 ⇒ a ≤ b − 1`). A satisfying
assignment exists exactly when `a ≠ b` over the integers — a false equality
satisfies neither arm — so the claim is sound, and the selector is forced to
the side the difference actually lies on.

The selector is a *free witness*: circuit-internal state that is not part of
the caller-facing witness API. The prover names only the declared inputs;
the driver solves the selector by search (`t` is tried as `0`, then `1`),
and every backend range-checks it against its declared 1-bit type regardless.

## Conditionals: `if`

An `if` statement lowers both branches and gates each branch's constraints
by a selector pinned to the condition's truth (`s` for the then-branch,
`1 − s` for the else-branch): in an unselected branch every constraint
becomes an identically-zero vacuity. Conditions may be any of the six
comparisons, including `==` and `!=`. An `if`/`else` *expression* multiplexes
the two branch values: `b + s·(a − b)`.

## Loops: bounded `for`

A `for` loop must iterate a literal range (`for i in 0..8`, `a..=b` forms
included) and is unrolled: the counter is a per-iteration constant and the
body lowers once per iteration. Unrolling is capped (1 024 iterations) to
keep circuits fixed-size and compile times sane. Combined with a `let mut`
accumulator this covers the classic sum/product patterns:

```rust
#[zk_provable(backend = "halo2")]
fn weighted_sum(#[secret] x: i64, #[public] total: i64) {
    let mut acc = 0;
    for i in 1..5 {
        acc += i * x;
    }
    assert_eq!(acc, total);
}
```

Each `acc += …` becomes a fresh SSA value; the loop counter never leaks past
the loop, and code after it reads the final accumulator value.

What the prover gets:

* `ProveAgeOverInputs` — a typed input struct, one field per parameter;
* `NamedWitness` — build the witness by *name*; resolution is checked
  (every slot present, nothing extra) before proving;
* the prove path re-validates the witness with exact `i128` arithmetic and
  rejects violations with a constraint index — and solves any gadget
  selectors first, so a false `!=` claim is refused before any proving work.

Soundness rails, enforced at compile/keygen time:

* every input is range-checked against its *declared* type (free selectors
  against their 1-bit type);
* intermediates are statically bounded below the field size, so field
  wraparound cannot satisfy a constraint integer arithmetic would not;
* division is the quotient/remainder gadget (non-negative remainder), and
  division by zero cannot prove.
