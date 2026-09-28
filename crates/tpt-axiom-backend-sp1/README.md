# tpt-axiom-backend-sp1

The [SP1](https://github.com/succinctlabs/sp1) adapter slot for
[`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom).

**Status: `ZkBackend` contract implemented, proving deferred.** SP1 is a
zkVM: instead of lowering the IR into a constraint system, a faithful adapter
emits a small RISC-V program that interprets the IR's expression table and
proves its execution with `sp1_sdk`. That work is blocked wherever the SP1
toolchain is unavailable — the `sp1-sdk` crate and the
`riscv32im-succinct-zkvm-elf` Rust target are only distributed for Linux and
macOS.

The `Sp1Backend` type implements the full `ZkBackend` contract so dependents
compile and the adapter's shape is fixed; every proving operation returns
`Sp1Error::Unavailable` with an explanatory message. The planned design is
documented in the crate docs: an IR-to-program lowering would live in a
future `tpt-axiom-sp1-program` crate (host-side encoder + guest-side
interpreter), keeping `sp1-sdk` out of this crate's dependency tree until
then.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
