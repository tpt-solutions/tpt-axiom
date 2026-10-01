# Introduction

`tpt-axiom` brings probabilistic reasoning and zero-knowledge proofs into
ordinary Rust: uncertain values propagate through `+`, `-`, `*`, `/`
automatically, plain functions become provable circuits with one attribute,
and decisions make abstention a first-class outcome.

Two lines capture the flavor:

```rust
let next_pos = pos + vel * dt;     // uncertainty propagates
assert!(sender_balance >= amount); // becomes a provable constraint
```

> **This crate has not been independently audited.** Read
> [Honest limits](limits.md) before relying on any proof it produces.

The authoritative references are the API docs
([docs.rs/tpt-axiom](https://docs.rs/tpt-axiom)) and the design document in
the repository (`docs/design/spec.md`). This book is the guided tour.
