use wasm_bindgen::{JsValue, wasm_bindgen};

pub(crate) fn test_jsvalue_constants() {
    // Test that undefined and null constants have correct identity checks
    let undef = JsValue::undefined();
    assert!(undef.is_undefined());
    assert!(!undef.is_null());

    let null = JsValue::null();
    assert!(null.is_null());
    assert!(!null.is_undefined());

    // Constants should be equal to themselves
    assert_eq!(JsValue::UNDEFINED, JsValue::undefined());
    assert_eq!(JsValue::NULL, JsValue::null());
    assert_eq!(JsValue::TRUE, JsValue::from_bool(true));
    assert_eq!(JsValue::FALSE, JsValue::from_bool(false));
}

pub(crate) fn test_jsvalue_bool() {
    // Test from_bool and as_bool
    let js_true = JsValue::from_bool(true);
    let js_false = JsValue::from_bool(false);

    assert_eq!(js_true.as_bool(), Some(true));
    assert_eq!(js_false.as_bool(), Some(false));

    // Non-bool values should return None
    assert_eq!(JsValue::undefined().as_bool(), None);
    assert_eq!(JsValue::null().as_bool(), None);
}

pub(crate) fn test_jsvalue_default() {
    // Default should be undefined
    let default: JsValue = Default::default();
    assert!(default.is_undefined());
    assert_eq!(default, JsValue::UNDEFINED);
}

pub(crate) fn test_jsvalue_clone_reserved() {
    // Cloning reserved values should not call JS (they're constants)
    let undef = JsValue::undefined();
    let undef_clone = undef.clone();
    assert!(undef_clone.is_undefined());
    assert_eq!(undef, undef_clone);

    let null = JsValue::null();
    let null_clone = null.clone();
    assert!(null_clone.is_null());
    assert_eq!(null, null_clone);

    let js_true = JsValue::from_bool(true);
    let true_clone = js_true.clone();
    assert_eq!(true_clone.as_bool(), Some(true));
    assert_eq!(js_true, true_clone);

    let js_false = JsValue::from_bool(false);
    let false_clone = js_false.clone();
    assert_eq!(false_clone.as_bool(), Some(false));
    assert_eq!(js_false, false_clone);
}

pub(crate) fn test_jsvalue_equality() {
    // Same values should be equal
    assert_eq!(JsValue::undefined(), JsValue::undefined());
    assert_eq!(JsValue::null(), JsValue::null());
    assert_eq!(JsValue::from_bool(true), JsValue::from_bool(true));
    assert_eq!(JsValue::from_bool(false), JsValue::from_bool(false));

    // Different values should not be equal
    assert_ne!(JsValue::undefined(), JsValue::null());
    assert_ne!(JsValue::from_bool(true), JsValue::from_bool(false));
    assert_ne!(JsValue::undefined(), JsValue::from_bool(false));
}

pub(crate) fn test_jsvalue_from_js() {
    // Test that JsValue can be returned from JS functions and checked with JsValue methods
    #[wasm_bindgen(inline_js = r#"
        export function get_undefined() { return undefined; }
        export function get_null() { return null; }
        export function get_object() { return { foo: "bar" }; }
    "#)]
    extern "C" {
        fn get_undefined() -> JsValue;
        fn get_null() -> JsValue;
        fn get_object() -> JsValue;
    }

    // Get values from JS and verify using JsValue methods
    let undef = get_undefined();
    eprintln!("[TEST] get_undefined() returned idx={:?}", undef);
    assert!(
        undef.is_undefined(),
        "get_undefined() should return undefined"
    );

    let null = get_null();
    eprintln!("[TEST] get_null() returned idx={:?}", null);
    assert!(null.is_null(), "get_null() should return null");

    let obj = get_object();
    eprintln!("[TEST] get_object() returned idx={:?}", obj);
    assert!(!obj.is_undefined(), "get_object() should NOT be undefined");
    assert!(!obj.is_null(), "get_object() should NOT be null");
}

pub(crate) fn test_jsvalue_as_string() {
    // Test from_str and as_string
    let js_str = JsValue::from_str("hello");
    assert_eq!(js_str.as_string(), Some("hello".to_string()));

    let js_num = JsValue::from_f64(42.0);
    assert_eq!(js_num.as_string(), None);
}

pub(crate) fn test_jsvalue_pass_to_js() {
    // Test passing Rust-created JsValue constants to JS
    #[wasm_bindgen(inline_js = r#"
        export function check_is_undefined(x) { return x === undefined; }
        export function check_is_null(x) { return x === null; }
    "#)]
    extern "C" {
        fn check_is_undefined(x: &JsValue) -> bool;
        fn check_is_null(x: &JsValue) -> bool;
    }

    // Test that Rust-created constants are correctly interpreted by JS
    assert!(
        check_is_undefined(&JsValue::undefined()),
        "JsValue::undefined() should be undefined in JS"
    );
    assert!(
        check_is_null(&JsValue::null()),
        "JsValue::null() should be null in JS"
    );
}
