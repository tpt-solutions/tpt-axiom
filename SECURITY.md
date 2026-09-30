# Security policy

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report privately:

1. Preferred: [GitHub private vulnerability reporting](https://github.com/tpt-solutions/tpt-axiom/security/advisories/new) on this repository.
2. Otherwise: email **security@tpt.solutions** with "tpt-axiom" in the subject.

Please include a description, reproduction steps or a proof of concept, the
affected crate(s) and version, and your assessment of impact and
exploitability. You will get an acknowledgment within 5 business days and a
status update at least every 14 days until resolution. We will credit
reporters in the release notes unless they prefer otherwise.

## Support status

Only the latest published release of each crate receives security fixes.
Pre-1.0 minor releases may fix security issues without ceremony; check the
changelog.

## Security model and current limits

**This crate has not been independently audited.** Read this section before
relying on any proof this crate produces.

### What the ZK layer claims to guarantee

For a circuit produced by `#[zk_provable]` and proved by the halo2 or
arkworks adapter:

* A verifying key accepts a proof only for public inputs that satisfy the
  circuit's constraints, over the declared integer types (each input is
  range-checked against its own width and signedness).
* Intermediate arithmetic is statically bounded below the field size at
  compile time (`ConstraintSystem::validate`), so a proof cannot exploit
  field wraparound to satisfy a constraint integer arithmetic would not.
* Witnesses are re-checked with exact `i128` evaluation before proving; a
  violating witness is rejected with a constraint index rather than proven.
* `ProofClaim` commits to a SHA-256 digest of the exact circuit IR; a proof
  cannot be replayed against a different circuit definition.

### Known limitations — read before production use

1. **No independent audit.** The implementations are new. An audit is a
   prerequisite for any production deployment; until then treat proofs as
   engineering-grade.
2. **Groth16 trusted setup (arkworks backend).** Groth16 uses a
   *per-circuit* setup whose randomness ("toxic waste") must be destroyed
   after key generation; anyone holding it can forge proofs. Keys generated
   by this crate use in-process `OsRng` with **no ceremony** — suitable for
   development, testing, and benchmarks only. Production needs a multi-party
   setup ceremony or a universally-updatable-setup system. The halo2 adapter
   (IPA) has no trusted setup.
3. **Verifier key provenance is out of scope.** This crate verifies proofs
   against a verifying key it is *given*. Binding a verifying key to an
   identity, a deployment, or a circuit registry is the integrator's job.
4. **Proof-of-execution, not proof-of-data.** Proofs attest that the
   *statement* holds for the *public inputs*; they say nothing about where
   the inputs came from. Garbage-in is provable garbage-out.
5. **Randomness.** Proving uses the OS RNG. Deterministic (non-malleable)
   proving is not provided; if your application needs proof
   non-malleability, handle it at the protocol layer.
6. **The intelligence types are trust-free by construction** (validated
   ranges, total functions) but say nothing about the *truth* of their
   inputs; `Provenance` metadata is producer-claimed, not verified.

### Threat model summary

| Adversary | Mitigated by |
| --- | --- |
| Prover submitting a witness violating the constraints | IR re-check at prove time + circuit constraints |
| Prover exploiting field wraparound / type confusion | static bit-width validation + declared-type range checks |
| Proof replayed against a different circuit | `ProofClaim` IR digest commitment |
| Verifier accepting a proof over wrong public inputs | copy/instance constraints; wrong public-input count is a clean rejection |
| Malicious verifying key substitution | **not mitigated** — key management is the integrator's responsibility |
| Toxic-waste holder forging Groth16 proofs | **not mitigated** in-development (no ceremony); use halo2 or run a ceremony |

## Cryptographic dependencies

halo2 proofs: `halo2_proofs` 0.3 (IPA over Pallas/Vesta). Groth16 proofs:
`ark-groth16` 0.5 over BLS12-381. The IR-digest commitment: SHA-256
(`sha2`). We track RUSTSEC advisories for all of these via `cargo deny` in
CI.
