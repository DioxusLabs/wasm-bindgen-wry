//! Tests for thread locals

use wasm_bindgen::wasm_bindgen;

#[wasm_bindgen(inline_js = "export var CONST = 42;")]
extern "C" {
    #[wasm_bindgen(thread_local_v2)]
    static CONST: f64;
}

pub(crate) fn test_thread_local() {
    // Access the thread local variable and verify its value
    let value = CONST.with(Clone::clone);
    assert_eq!(value, 42.0);
}
