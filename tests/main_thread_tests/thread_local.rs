//! Tests for thread locals

use wasm_bindgen::wasm_bindgen;

#[wasm_bindgen(inline_js = "export var CONST = 42;")]
extern "C" {
    #[wasm_bindgen(thread_local_v2)]
    static CONST: f64;
    #[wasm_bindgen(thread_local_v2, js_name = window)]
    static WINDOW: Option<wasm_bindgen::JsValue>;
}

pub(crate) fn test_thread_local() {
    // Access the thread local variable and verify its value
    let value = CONST.with(Clone::clone);
    assert_eq!(value, 42.0);
}

pub(crate) fn test_thread_local_window() {
    // Access the thread local window variable and verify it's not null
    let window = WINDOW.with(Clone::clone);
    assert!(window.is_some(), "Expected window to be Some");
}