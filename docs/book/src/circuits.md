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
bindings, `assert!`/`assert_eq!` (conjunctions with `&&`), and
`debug_assert!` (a circuit has no debug builds — it is always enforced).
Everything else is a clear compile error, including early returns and
signed division.

What the prover gets:

* `ProveAgeOverInputs` — a typed input struct, one field per parameter;
* `NamedWitness` — build the witness by *name*; resolution is checked
  (every slot present, nothing extra) before proving;
* the prove path re-validates the witness with exact `i128` arithmetic and
  rejects violations with a constraint index.

Soundness rails, enforced at compile/keygen time:

* every input is range-checked against its *declared* type;
* intermediates are statically bounded below the field size, so field
  wraparound cannot satisfy a constraint integer arithmetic would not;
* division is the quotient/remainder gadget (non-negative remainder), and
  division by zero cannot prove.
