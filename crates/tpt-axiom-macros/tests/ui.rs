//! Compile-fail (UI) tests: `#[zk_provable]` must reject unsupported Rust
//! constructs with a clear, stable compiler error rather than a confusing
//! failure deep in generated code.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
