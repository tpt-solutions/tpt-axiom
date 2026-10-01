# Proof envelopes & services

A proof is only useful if it can travel. The
[`ProofEnvelope`](https://docs.rs/tpt-axiom-zk/latest/tpt_axiom_zk/struct.ProofEnvelope.html)
is the wire format: version tag, backend name, circuit name, a SHA-256
**digest commitment to the exact circuit IR**, the public inputs, and the
canonical proof bytes. `ProofClaim::to_envelope` / `from_envelope`
round-trip a claim through JSON with the digest intact — a rebuilt claim
still verifies, and still *refuses* a proof replayed against a different
circuit definition.

The `proof_service` example (in the examples crate) shows the deployment shape: a prover service
(name-keyed JSON witness in, envelope out — the secret witness never leaves
that process), a verifier service (envelope in, verdict out), and a client
watching a tampered envelope get refused. The CLI (`cargo axiom prove
--input witness.json`, `cargo axiom verify --proof envelope.json`) speaks
the same formats.
