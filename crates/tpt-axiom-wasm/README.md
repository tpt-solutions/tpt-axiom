# tpt-axiom-wasm

WASM bindings behind the browser playground (`docs/playground/`). Build with:

```sh
wasm-pack build crates/tpt-axiom-wasm --target web --out-dir ../../docs/playground/pkg
```

Then serve `docs/playground/` statically (e.g. `python -m http.server`) and
open it in a browser.
