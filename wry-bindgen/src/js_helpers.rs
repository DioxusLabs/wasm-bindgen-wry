//! Javascript methods defined for use in JsValue methods

use crate::JsValue;
use crate::wasm_bindgen;

#[wasm_bindgen(crate = crate, inline_js = include_str!("./js/convert.js"))]
extern "C" {
    #[wasm_bindgen(js_name = "__wry_is_undefined")]
    pub(crate) fn js_is_undefined(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_null")]
    pub(crate) fn js_is_null(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_true")]
    pub(crate) fn js_is_true(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_false")]
    pub(crate) fn js_is_false(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_typeof")]
    pub(crate) fn js_typeof(x: &JsValue) -> String;

    #[wasm_bindgen(js_name = "__wry_is_falsy")]
    pub(crate) fn js_is_falsy(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_truthy")]
    pub(crate) fn js_is_truthy(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_object")]
    pub(crate) fn js_is_object(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_function")]
    pub(crate) fn js_is_function(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_string")]
    pub(crate) fn js_is_string(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_symbol")]
    pub(crate) fn js_is_symbol(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "__wry_is_bigint")]
    pub(crate) fn js_is_bigint(x: &JsValue) -> bool;

    /// Get the string value of a JsValue if it is a string, otherwise None.
    #[wasm_bindgen(js_name = "__wry_as_string")]
    pub(crate) fn js_as_string(x: &JsValue) -> Option<String>;

    /// Create a JsValue from a string.
    #[wasm_bindgen(js_name = "__wry_str_to_jsvalue")]
    pub(crate) fn js_string_to_jsvalue(s: &str) -> JsValue;

    /// Create a JsValue from a float.
    #[wasm_bindgen(js_name = "__wry_float_to_jsvalue")]
    pub(crate) fn js_float_to_jsvalue(n: f64) -> JsValue;
}
