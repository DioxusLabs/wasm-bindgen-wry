use wasm_bindgen::{JsCast, JsValue, wasm_bindgen};

// Test that is_type_of can be used for custom type checking
// This is particularly useful for primitive types that don't work with instanceof

/// A wrapper type for JavaScript strings that uses is_type_of instead of instanceof
/// because primitive strings don't pass instanceof String checks
#[wasm_bindgen(inline_js = r#"
    export function create_string() { return "hello"; }
    export function create_number() { return 42; }
    export function create_object() { return {}; }
"#)]
extern "C" {
    // Custom type that uses is_type_of for checking string primitives
    #[wasm_bindgen(is_type_of = |val: &JsValue| val.is_string())]
    type JsString;

    // Custom type that uses is_type_of for checking number primitives
    #[wasm_bindgen(is_type_of = |val: &JsValue| val.as_f64().is_some())]
    type JsNumber;

    fn create_string() -> JsValue;
    fn create_number() -> JsValue;
    fn create_object() -> JsValue;
}

pub(crate) fn test_is_type_of_string() {
    let str_val = create_string();
    let num_val = create_number();
    let obj_val = create_object();

    // JsString::is_type_of should return true for strings
    assert!(
        JsString::is_type_of(&str_val),
        "is_type_of should return true for string"
    );

    // JsString::is_type_of should return false for non-strings
    assert!(
        !JsString::is_type_of(&num_val),
        "is_type_of should return false for number"
    );
    assert!(
        !JsString::is_type_of(&obj_val),
        "is_type_of should return false for object"
    );
}

pub(crate) fn test_is_type_of_number() {
    let str_val = create_string();
    let num_val = create_number();
    let obj_val = create_object();

    // JsNumber::is_type_of should return true for numbers
    assert!(
        JsNumber::is_type_of(&num_val),
        "is_type_of should return true for number"
    );

    // JsNumber::is_type_of should return false for non-numbers
    assert!(
        !JsNumber::is_type_of(&str_val),
        "is_type_of should return false for string"
    );
    assert!(
        !JsNumber::is_type_of(&obj_val),
        "is_type_of should return false for object"
    );
}

pub(crate) fn test_is_type_of_with_dyn_into() {
    let str_val = create_string();
    let num_val = create_number();

    // dyn_into should use is_type_of for type checking
    let str_result: Result<JsString, _> = str_val.dyn_into();
    assert!(
        str_result.is_ok(),
        "dyn_into should succeed for string using is_type_of"
    );

    // dyn_into should fail for wrong type
    let num_result: Result<JsString, _> = num_val.dyn_into();
    assert!(
        num_result.is_err(),
        "dyn_into should fail for non-string using is_type_of"
    );
}

pub(crate) fn test_is_type_of_with_dyn_ref() {
    let str_val = create_string();
    let num_val = create_number();

    // dyn_ref should use is_type_of for type checking
    let str_ref: Option<&JsString> = str_val.dyn_ref();
    assert!(
        str_ref.is_some(),
        "dyn_ref should return Some for string using is_type_of"
    );

    // dyn_ref should return None for wrong type
    let num_ref: Option<&JsString> = num_val.dyn_ref();
    assert!(
        num_ref.is_none(),
        "dyn_ref should return None for non-string using is_type_of"
    );
}

pub(crate) fn test_has_type_with_is_type_of() {
    let str_val = create_string();
    let num_val = create_number();

    // has_type should use is_type_of
    assert!(
        str_val.has_type::<JsString>(),
        "has_type should return true for string using is_type_of"
    );
    assert!(
        !str_val.has_type::<JsNumber>(),
        "has_type should return false for string when checking for number"
    );

    assert!(
        num_val.has_type::<JsNumber>(),
        "has_type should return true for number using is_type_of"
    );
    assert!(
        !num_val.has_type::<JsString>(),
        "has_type should return false for number when checking for string"
    );
}
