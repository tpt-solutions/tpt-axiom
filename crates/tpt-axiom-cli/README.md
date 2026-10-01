# tpt-axiom-cli

The [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) build-time
driver, installed as the `axiom` binary.

Later phases use this binary to generate proving/verifying keys for
`#[zk_provable]` circuits, invoke verification (including the Phase 3
circuit-equivalence checks in `tpt-axiom-verify`), and drive the proving backends.

## Status: Phase 0 scaffolding

The command surface exists so scripts and CI can call `axiom <command>`
today; the commands themselves are stubs that exit non-zero with a clear
"not implemented yet" message.

```text
tpt-axiom-cli 0.1.0 — tpt-axiom build-time driver

USAGE:
    axiom <COMMAND>

COMMANDS:
    keys        Generate proving/verifying keys for a circuit
    verify      Verify a proof (and, later, Rust <-> circuit equivalence)
    version     Print the version
    help        Print this help
```

## Install

```sh
cargo install --path crates/tpt-axiom-cli
```

## Proving backends are feature-gated

This crate has **no default features**, so a plain build has no proving backend
and `prove`/`keygen` return a usage error telling you to rebuild with
`--features halo2`. `cargo axiom doctor` lists what the current build has.

| Feature     | Enables                                     |
| ----------- | ------------------------------------------- |
| `halo2`     | `tpt-axiom-backend-halo2` (IPA/Vesta)       |
| `arkworks`  | `tpt-axiom-backend-arkworks` (Groth16)      |
| `full`      | both of the above                           |

The end-to-end `prove`/`keygen` tests in `tests/cli.rs` are gated on
`#[cfg(feature = "halo2")]`, so to run them:

```sh
cargo test -p tpt-axiom-cli --features halo2
```

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
