# tpt-axiom-template

A [`cargo generate`](https://cargo-generate.github.io/cargo-generate/) template
for new projects built on `tpt-axiom` — uncertainty-propagating arithmetic,
`#[zk_provable]` circuits, and a real proof round-trip, ready to extend.

## Usage

```sh
cargo install cargo-generate
cargo generate tpt-solutions/tpt-axiom --path crates/tpt-axiom-template
# answer the prompts (project name), then:
cd <project-name>
cargo run
```

## What you get

* a dependency on the `tpt-axiom` umbrella crate (halo2 backend feature on);
* `src/main.rs` with two worked starters — a `Fuzzy` sensor-fusion
  computation and the `prove_age_over` circuit proven and verified through
  the halo2 backend;
* `cargo test` ready (the circuit's prove/verify round-trip runs as a test).
