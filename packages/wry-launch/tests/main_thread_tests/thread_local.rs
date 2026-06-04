//! Tests for thread locals

use wasm_bindgen::{JsCast, wasm_bindgen};
use wry_launch::JsValue;

#[wasm_bindgen(inline_js = "export var CONST = 42;")]
extern "C" {
    #[wasm_bindgen(thread_local_v2)]
    static CONST: f64;
}
#[wasm_bindgen]
extern "C" {
    #[derive(Clone)]
    type Window;

    #[wasm_bindgen(thread_local_v2, js_name = window)]
    static WINDOW: Option<Window>;
}

pub(crate) fn test_thread_local() {
    // Access the thread local variable and verify its value
    let value = CONST.with(Clone::clone);
    assert_eq!(value, 42.0);
}

pub(crate) fn test_thread_local_window() {
    // Access the thread local window variable and verify it's not null
    let window = WINDOW.with(Clone::clone);
    let window = window.expect("Expected window to be Some");
    assert!(window.is_object(), "Expected window to be an object");
    let as_js_value: &JsValue = window.as_ref();
    assert!(
        as_js_value.clone().dyn_into::<Window>().is_ok(),
        "Expected window to be a Window instance"
    );
}
