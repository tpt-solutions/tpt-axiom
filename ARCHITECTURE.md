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

## Writing a backend

The `ZkBackend` trait (`tpt-axiom-zk`) is the entire integration surface; a
new proving stack plugs in behind it without touching the macro, the IR, or
any downstream crate. The two shipping adapters (`tpt-axiom-backend-halo2`
and `tpt-axiom-backend-arkworks`) are the reference implementations; the
`custom_backend` example (`crates/tpt-axiom-zk/examples/custom_backend.rs`)
is a minimal skeleton over the reference R1CS lowering.

The contract, in the order a backend must honor it:

1. **`compile(ir)`** — lower the `ConstraintSystem` into the backend's native
   circuit form. Call `ir.validate()` first and surface
   [`CircuitError`](tpt-axiom-ir) rather than panicking: a malformed IR must
   be a typed error, never a synthesis-time panic. The IR is a DAG of `+`,
   `-`, `*`, negation, and the gateless `div_trunc` node (see below), with
   three constraint kinds — `Equal`, `Zero`, `NonNegative` — and *free
   (aux) witness variables* (see step 3).

2. **`generate_keys(ir, params)`** — produce `(ProvingKey, VerifyingKey)` at
   the configured size. `KeygenOptions` encodes into the `params` bytes the
   trait already speaks (`range_bits`, and halo2's row bound `k`); decode
   them through the backend's `*Params` type as the shipping adapters do,
   and auto-size rather than panic when no size is given. Reject any
   intermediate whose static bit bound reaches `MAX_FAITHFUL_BITS` — that
   check is what keeps field arithmetic a faithful image of integer
   arithmetic.

3. **`prove(circuit, pk, public, secret)`** — the soundness-critical half.
   The `secret` slice carries the declared secrets *followed by the circuit's
   solved free-witness values* (the selector bits of `!=`/`if` gadgets, in
   `ir.free_variables()` order); for a circuit without free variables it is
   exactly the named secrets, so the tail convention costs nothing. Enforce
   the arity (`secret_inputs.len() + ir.num_free()`), re-run
   `witness::check_with_range` (an unsatisfying witness must be rejected
   before proving — real provers otherwise happily prove false statements),
   then synthesize. Range semantics every backend must reproduce:
   each named input is decomposed in `int_type.bits` bits (signed types
   shifted by `2^(bits−1)`), each `NonNegative` in `range_bits` bits, free
   variables in their declared 1-bit type — with the decomposed sum tied
   back to the constrained node's own cell/variable, so a negative value's
   field wrap is rejected rather than silently re-ranged.

4. **`verify(vk, public, proof)`** — check the proof against the publics
   alone. A malformed proof is a clean `Ok(false)`, not an error, wherever
   the backend can distinguish the two; a wrong public-input count is also
   `Ok(false)` (halo2's verifying material carries `num_publics` because its
   vk cannot recover it; arkworks derives the count from the vk).

5. **`encode_proof`/`decode_proof`** (optional) — canonical proof bytes so
   `ProofEnvelope` can carry the claim across process/network boundaries.
   Decode must refuse trailing bytes rather than verify a prefix.

The `div_trunc` node deserves a note because it is the template for any
future free-witness gadget: it carries *no gate of its own* in either
backend. Its soundness lives entirely in the quotient/remainder gadget's
constraints (`dividend == quotient·divisor + remainder`, `remainder ≥ 0`,
`divisor − remainder − 1 ≥ 0`) — the node just transports the prover's
answer, and the polynomial constraints make forging it unsatisfiable.
Likewise the `!=`/`if` gadgets are pure constraint shapes over ordinary
`Mul`/`Sub` nodes plus free boolean selectors; nothing about them is
backend-specific.

Finally, run the shared conformance suite —
`tpt_axiom_zk::conformance::run_all(&YourBackend)` in the adapter's test
suite — which drives the same scenarios (`balance_transfer`, `weighted_sum`,
`bounds_check`, and the free-witness `not_equal` gadget) through every
backend and asserts identical verdicts on the happy paths, tampered
publics, and violating witnesses. A backend that passes `run_all` behaves,
verdict-for-verdict, like the shipping two.

## Threat model

The security-relevant guarantees and their current limits (trusted-setup
status, key provenance being out of scope, the not-yet-audited status) are
owned by [SECURITY.md](SECURITY.md); that document, not this one, is the
authoritative statement. Architecturally, the trust boundary is the
`ZkBackend::verify` call: everything upstream of it (probabilistic types,
IR construction, witness preparation) is untrusted computation, and the
verifier consumes only the verifying key, the public inputs, and the proof.
