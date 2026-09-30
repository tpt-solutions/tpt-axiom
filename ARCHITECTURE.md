# Architecture

This document captures the high-level architecture from [docs/design/spec.md](docs/design/spec.md)
§3 and maps it onto the crate layout in this workspace.

## Data/verification flow

```text
[ Rust Application Logic (Probabilistic / ZK) ]
       │
       ▼
[ Mathematical IR Generator ] ──(Arithmetic Constraints)──> [ tpt-axiom-verify ]
       │ (Checks circuit equivalence by polynomial identity)
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
- **`tpt-axiom-verify`** checks that IR against reference semantics by exact
  polynomial identity — that a lowered ZK circuit encodes the comparison the
  annotated function stated (this is what catches "circuit mismatch" bugs),
  and that the variance-propagation formulas match the textbook identities
  where such a check is meaningful (see that crate's docs for the honest
  limits: a transcription of a two-term sum verifies nothing).
  (`tpt-telos` was investigated for this role and superseded: its solver is
  linear-arithmetic-only, while the IR and the variance formulas are
  genuinely nonlinear; `tpt-axiom-verify`'s canonical-polynomial checker
  covers those cases with no external dependency.)
- **The optimized output** is either ordinary compiled Rust (for the
  probabilistic path) or a concrete R1CS/PLONKish circuit plus proving and
  verifying keys, produced by a backend adapter (`tpt-axiom-backend-halo2`,
  `tpt-axiom-backend-arkworks`, `tpt-axiom-backend-sp1`) implementing the `ZkBackend`
  trait from `tpt-axiom-zk`.

## Crate dependency graph

```text
                       ┌────────────────┐
                       │ tpt-axiom-core │  (Fuzzy<T>, Distribution<T>)
                       └───────┬────────┘
                               │
                         ┌─────▼──────┐
                         │ tpt-axiom  │  (umbrella crate / prelude)
                         └────────────┘

┌─────────────────┐     ┌──────────────┐     ┌─────────────────────────┐
│tpt-axiom-macros │────>│ tpt-axiom-ir │<────│ tpt-axiom-zk (ZkBackend) │
└─────────────────┘     └──────┬───────┘     └────────────┬─────────────┘
                                │                           │
                                │           ┌───────────────┼─────────────────────────┐
                                │           ▼                ▼                         ▼
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
3. **Verification** — `tpt-axiom-verify`: the verification arrow in the
   diagram above, catching circuit-mismatch lowering bugs by exact
   polynomial identity (`cargo test -p tpt-axiom-verify`).
4. **Phase 4** — `tpt-axiom-backend-*`: concrete `ZkBackend` implementations
   producing real proofs.

## Threat model

The security-relevant guarantees and their current limits (trusted-setup
status, key provenance being out of scope, the not-yet-audited status) are
owned by [SECURITY.md](SECURITY.md); that document, not this one, is the
authoritative statement. Architecturally, the trust boundary is the
`ZkBackend::verify` call: everything upstream of it (probabilistic types,
IR construction, witness preparation) is untrusted computation, and the
verifier consumes only the verifying key, the public inputs, and the proof.
