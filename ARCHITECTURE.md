# Architecture

This document captures the high-level architecture from [spec.txt](spec.txt)
§3 and maps it onto the crate layout in this workspace.

## Data/verification flow

```text
[ Rust Application Logic (Probabilistic / ZK) ]
       │
       ▼
[ Mathematical IR Generator ] ──(Arithmetic Constraints)──> [ tpt-telos SMT ]
       │ (Verifies variance propagation & circuit equivalence)
       ▼
[ Optimized Probabilistic Code / ZK Circuit (R1CS/PLONK) ]
```

- **Rust application logic** is ordinary Rust using `Fuzzy<T>` / `Distribution<T>`
  arithmetic (probabilistic path) and/or `#[zk_provable]`-annotated functions
  (ZK path).
- **The mathematical IR generator** (`tpt-axiom-ir`) lowers both paths into a
  shared, backend-agnostic constraint graph: variance-propagation constraints
  for probabilistic arithmetic, and arithmetic gate/constraint nodes for ZK
  circuits.
- **`tpt-telos`** (Phase 3, integrated as a workspace dependency) formally
  verifies that IR against the original Rust semantics — that propagated
  variance formulas are sound, and that a lowered ZK circuit is equivalent to
  the asserted constraints in the annotated function body. This is what
  catches "circuit mismatch" bugs before they reach compiled output.
- **The optimized output** is either ordinary compiled Rust (for the
  probabilistic path) or a concrete R1CS/PLONKish circuit plus proving and
  verifying keys, produced by a backend adapter (`tpt-axiom-backend-halo2`,
  `tpt-axiom-backend-arkworks`, `tpt-axiom-backend-sp1`) implementing the `ZkBackend`
  trait from `tpt-axiom-zk`.

## Crate dependency graph

```text
                         ┌──────────────────┐
                         │  tpt-axiom-core   │  (Fuzzy<T>, Distribution<T>)
                         └─────────┬─────────┘
                                   │
                         ┌─────────▼─────────┐
                         │     tpt-axiom     │  (umbrella crate / prelude)
                         └───────────────────┘

┌───────────────────┐     ┌────────────────┐     ┌──────────────────────────┐
│  tpt-axiom-macros  │────>│  tpt-axiom-ir  │<────│  tpt-axiom-zk (ZkBackend) │
└───────────────────┘     └───────┬────────┘     └─────────────┬────────────┘
                                   │                             │
                                   │            ┌────────────────┼──────────────────┐
                                   │            ▼                ▼                   ▼
                                   │  tpt-axiom-backend-halo2  tpt-axiom-backend-arkworks  tpt-axiom-backend-sp1
                                   │
                                   └──> consumed by tpt-axiom-cli (build-time driver)
```

- `tpt-axiom-core` has no dependency on the ZK stack — probabilistic types remain
  usable (and publishable) independently of zero-knowledge concerns.
- `tpt-axiom-ir` is the single shared IR consumed by both the macro layer
  (`tpt-axiom-macros`) and the backend abstraction layer (`tpt-axiom-zk` and its
  adapters), so a circuit produced by the macro is guaranteed to be
  structurally compatible with every backend.
- `tpt-axiom` re-exports the stable public surface (`prelude`) so downstream
  crates only ever need one dependency.

## Phased delivery

Each phase in [todo.md](todo.md) corresponds to a layer of this diagram:

1. **Phase 1** — `tpt-axiom-core` (top box): `Fuzzy<T>`/`Distribution<T>` and
   error-propagation arithmetic. No IR, no ZK.
2. **Phase 2** — `tpt-axiom-ir` + `tpt-axiom-macros`: lowering Rust to the shared IR,
   with `tpt-axiom-zk`'s `ZkBackend` trait defined as the consumption contract.
3. **Phase 3** — `tpt-telos` integration: the verification arrow in the
   diagram above becomes real, catching circuit-mismatch and unsound
   variance-propagation bugs at build time.
4. **Phase 4** — `tpt-axiom-backend-*`: concrete `ZkBackend` implementations
   producing real proofs.
