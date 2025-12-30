//! Javascript methods defined for use in JsValue methods

use alloc::string::String;

use crate::JsValue;
use crate::wasm_bindgen;

#[wasm_bindgen(crate = crate, inline_js = include_str!("./js/convert.js"))]
extern "C" {
    #[wasm_bindgen(js_name = "is_undefined")]
    pub(crate) fn js_is_undefined(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_null")]
    pub(crate) fn js_is_null(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_true")]
    pub(crate) fn js_is_true(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_false")]
    pub(crate) fn js_is_false(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "get_typeof")]
    pub(crate) fn js_typeof(x: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "is_falsy")]
    pub(crate) fn js_is_falsy(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_truthy")]
    pub(crate) fn js_is_truthy(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_object")]
    pub(crate) fn js_is_object(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_function")]
    pub(crate) fn js_is_function(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_string")]
    pub(crate) fn js_is_string(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_symbol")]
    pub(crate) fn js_is_symbol(x: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "is_bigint")]
    pub(crate) fn js_is_bigint(x: &JsValue) -> bool;

    /// Get the string value of a JsValue if it is a string, otherwise None.
    #[wasm_bindgen(js_name = "as_string")]
    pub(crate) fn js_as_string(x: &JsValue) -> Option<String>;

    /// Get the f64 value of a JsValue if it is a number, otherwise None.
    #[wasm_bindgen(js_name = "as_f64")]
    pub(crate) fn js_as_f64(x: &JsValue) -> Option<f64>;

    /// Get a debug string representation of the JsValue.
    #[wasm_bindgen(js_name = "debug_string")]
    pub(crate) fn js_debug_string(x: &JsValue) -> String;

    // Arithmetic operators
    #[wasm_bindgen(js_name = "js_checked_div")]
    pub(crate) fn js_checked_div(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_pow")]
    pub(crate) fn js_pow(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_add")]
    pub(crate) fn js_add(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_sub")]
    pub(crate) fn js_sub(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_mul")]
    pub(crate) fn js_mul(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_div")]
    pub(crate) fn js_div(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_rem")]
    pub(crate) fn js_rem(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_neg")]
    pub(crate) fn js_neg(a: &JsValue) -> JsValue;

    // Bitwise operators
    #[wasm_bindgen(js_name = "js_bit_and")]
    pub(crate) fn js_bit_and(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_bit_or")]
    pub(crate) fn js_bit_or(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_bit_xor")]
    pub(crate) fn js_bit_xor(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_bit_not")]
    pub(crate) fn js_bit_not(a: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_shl")]
    pub(crate) fn js_shl(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_shr")]
    pub(crate) fn js_shr(a: &JsValue, b: &JsValue) -> JsValue;

    #[wasm_bindgen(js_name = "js_unsigned_shr")]
    pub(crate) fn js_unsigned_shr(a: &JsValue, b: &JsValue) -> u32;

    // Comparison operators
    #[wasm_bindgen(js_name = "js_lt")]
    pub(crate) fn js_lt(a: &JsValue, b: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "js_le")]
    pub(crate) fn js_le(a: &JsValue, b: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "js_gt")]
    pub(crate) fn js_gt(a: &JsValue, b: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "js_ge")]
    pub(crate) fn js_ge(a: &JsValue, b: &JsValue) -> bool;

    #[wasm_bindgen(js_name = "js_loose_eq")]
    pub(crate) fn js_loose_eq(a: &JsValue, b: &JsValue) -> bool;

    // Other operators
    #[wasm_bindgen(js_name = "js_in")]
    pub(crate) fn js_in(prop: &JsValue, obj: &JsValue) -> bool;

    // instanceof check for Error
    #[wasm_bindgen(js_name = "is_error")]
    pub(crate) fn js_is_error(x: &JsValue) -> bool;

    // Heap management - clone a value in the JS heap
    #[wasm_bindgen(js_name = "clone_heap_ref")]
    pub(crate) fn js_clone_heap_ref(heap_id: u64) -> JsValue;

    // Heap management - drop a value from the JS heap
    #[wasm_bindgen(js_name = "drop_heap_ref")]
    pub(crate) fn js_drop_heap_ref(heap_id: u64);
}
