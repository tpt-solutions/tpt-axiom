# tpt-axiom-cli

The [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom) build-time
driver, installed as the `axiom` binary.

Later phases use this binary to generate proving/verifying keys for
`#[zk_provable]` circuits, invoke verification (including the Phase 3
`tpt-telos` circuit-equivalence check), and drive the Phase 4 backends.

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

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
