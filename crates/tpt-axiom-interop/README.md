# tpt-axiom-interop

Conversion interfaces between [`tpt-axiom`](https://github.com/tpt-solutions/tpt-axiom)
and the rest of the TPT AI ecosystem.

- **`augur`** — bridges [`tpt-augur`](https://github.com/tpt-solutions/tpt-augur)
  (via the published `tpt-augur-std` crate) to Axiom's uncertainty types:
  every `Dist` variant converts to a `Fuzzy<f64>` through its exact
  closed-form moments (information-preserving for `Normal`, which also
  round-trips back), with loud errors for out-of-domain parameters.
- **`inference`** — `InferenceSample` is the boundary contract for the TPT
  inference runtimes (`tpt-gpu`, `tpt-local-ai`, `tpt-spark`): a sampled
  value plus a log-space weight converts into weighted `Evidence` or a
  confidence-carrying `Score`. The runtimes are not published yet; their
  `From` impls will be feature-gated additions here.

Ecosystem coupling is deliberately isolated in this crate — `tpt-axiom-core`
never depends on it.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at your option.
