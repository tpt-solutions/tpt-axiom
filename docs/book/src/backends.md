# Proving backends

The same IR proves on every backend through the `ZkBackend` trait:

| Backend | Stack | Setup | Notes |
| --- | --- | --- | --- |
| halo2 | IPA over Pallas/Vesta | none (universal) | fastest proving; proof = transcript bytes |
| arkworks | Groth16 over BLS12-381 | **per-circuit trusted setup** | smallest proofs; canonical key serialization |
| SP1 | zkVM (RISC-V) | n/a | contract scaffolded; toolchain is Linux/macOS-only |

Reference numbers for the balance-transfer circuit on a dev workstation:
halo2 keygen ≈ 17 ms, prove ≈ 8 ms, verify ≈ 3 ms; arkworks keygen ≈ 414
ms, prove ≈ 338 ms, verify ≈ 10 ms. `cargo bench` is the authoritative
comparison.

**Groth16's toxic waste**: `generate_keys` samples fresh randomness that
must be destroyed after setup; anyone holding it can forge proofs. The
in-process `OsRng` setup is development-grade — production needs a
multi-party ceremony. See [Honest limits](limits.md).

The `custom_backend` example in `tpt-axiom-zk` walks through the whole
`ZkBackend` contract for adapter authors, using the reference R1CS lowering
in place of real cryptography.
