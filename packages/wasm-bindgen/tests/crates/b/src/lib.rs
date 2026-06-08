//! Fixture crate `b` for the upstream `duplicate_deps` test. Mirrors upstream's
//! `tests/crates/b`, but points the `module` path at the vendored companion JS
//! and uses the wry-bindgen shim.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "../../../../../../vendored/wasm-bindgen/tests/wasm/duplicate_deps.js")]
extern "C" {
    fn foo(x: u32);
}

pub fn test() {
    foo(10);
}
