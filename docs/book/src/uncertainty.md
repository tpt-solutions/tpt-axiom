# Uncertainty arithmetic

[`Fuzzy<T>`](https://docs.rs/tpt-axiom-core/latest/tpt_axiom_core/struct.Fuzzy.html)
is a value plus a variance, with the standard first-order error-propagation
rules bound to the arithmetic operators:

| Operation         | Mean        | Variance                        |
| ----------------- | ----------- | ------------------------------- |
| `a + b` / `a - b` | `m_a ± m_b` | `v_a + v_b`                     |
| `a * b`           | `m_a·m_b`   | `v_a v_b + v_a m_b² + v_b m_a²` |
| `a / b`           | `m_a/m_b`   | delta-method approximation      |
| `a * c`           | `c·m_a`     | `c² v_a`                        |

Three things to internalize (all documented on the type):

* operands are assumed **independent** — see
  [Correlated uncertainty](correlated.md) for the exact alternative;
* division is the **delta-method approximation**, not an identity;
* `x - x` has variance `2v`, because the model cannot see that both
  operands are the same quantity.

Nonlinear transforms exist as named methods (`exp`, `ln`, `sqrt`, `powi`,
`tanh`) with second-order mean correction, and the generic engine behind
them is `Fuzzy::transform(f, f', f'')`.

Helpers ride along: `standard_deviation`, `confidence_interval(level)`,
`z_score`, `checked_div`, `checked_fuse` (N-way fusion via `fuse_all`).
Degenerate arithmetic follows IEEE semantics instead of panicking; the
`checked_*` forms return `Result` when you want validation.
