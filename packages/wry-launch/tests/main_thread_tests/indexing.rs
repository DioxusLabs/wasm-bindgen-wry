use wasm_bindgen::prelude::*;

// Define Array type with indexing operations
#[wasm_bindgen]
extern "C" {
    pub type Array;

    #[wasm_bindgen(constructor)]
    fn new() -> Array;

    #[wasm_bindgen(method)]
    fn push(this: &Array, value: JsValue) -> u32;

    #[wasm_bindgen(method, getter)]
    fn length(this: &Array) -> u32;

    #[wasm_bindgen(method, structural, indexing_getter)]
    fn get(this: &Array, index: u32) -> JsValue;

    #[wasm_bindgen(method, structural, indexing_setter)]
    fn set(this: &Array, index: u32, value: JsValue);

    #[wasm_bindgen(method, structural, indexing_deleter)]
    fn delete(this: &Array, index: u32);
}

pub(crate) fn test_indexing_getter_array() {
    let arr = Array::new();

    // Push some values
    arr.push(JsValue::from(10));
    arr.push(JsValue::from(20));
    arr.push(JsValue::from(30));

    assert_eq!(arr.length(), 3);

    // Test indexing getter
    let first = arr.get(0);
    let second = arr.get(1);
    let third = arr.get(2);

    assert_eq!(first.as_f64(), Some(10.0));
    assert_eq!(second.as_f64(), Some(20.0));
    assert_eq!(third.as_f64(), Some(30.0));

    // Test out of bounds returns undefined
    let out_of_bounds = arr.get(100);
    assert!(out_of_bounds.is_undefined());
}

pub(crate) fn test_indexing_setter_array() {
    let arr = Array::new();

    // Push some values
    arr.push(JsValue::from(10));
    arr.push(JsValue::from(20));
    arr.push(JsValue::from(30));

    // Test indexing setter
    arr.set(1, JsValue::from(200));

    assert_eq!(arr.get(0).as_f64(), Some(10.0));
    assert_eq!(arr.get(1).as_f64(), Some(200.0));
    assert_eq!(arr.get(2).as_f64(), Some(30.0));

    // Set beyond bounds - should extend array
    arr.set(5, JsValue::from(500));
    assert_eq!(arr.length(), 6);
    assert_eq!(arr.get(5).as_f64(), Some(500.0));
    assert!(arr.get(4).is_undefined()); // Gap should be undefined
}

pub(crate) fn test_indexing_deleter_array() {
    let arr = Array::new();

    // Push some values
    arr.push(JsValue::from(10));
    arr.push(JsValue::from(20));
    arr.push(JsValue::from(30));

    // Delete middle element
    arr.delete(1);

    // Array length should still be 3, but element at index 1 should be undefined
    assert_eq!(arr.length(), 3);
    assert_eq!(arr.get(0).as_f64(), Some(10.0));
    assert!(arr.get(1).is_undefined()); // Deleted element is undefined
    assert_eq!(arr.get(2).as_f64(), Some(30.0));
}
