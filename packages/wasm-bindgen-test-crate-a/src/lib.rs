//! Fixture crate `a` for the upstream `duplicate_deps` test. Both this crate and
//! `wasm-bindgen-test-crate-b` import a `foo` binding from the *same* JS module
//! (`duplicate_deps.js`), exercising that two crates can depend on the same JS
//! dependency. Mirrors `vendored/wasm-bindgen/tests/crates/a`, but points the
//! `module` path at the vendored companion JS and uses the wry-bindgen shim.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "../../../vendored/wasm-bindgen/tests/wasm/duplicate_deps.js")]
extern "C" {
    fn foo();
}

pub fn test() {
    foo();
}
