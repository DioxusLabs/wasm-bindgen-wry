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

pub(crate) fn test_jsvalue_as_f64() {
    // Test as_f64 with numbers
    #[wasm_bindgen(inline_js = r#"
        export function get_number(n) { return n; }
    "#)]
    extern "C" {
        fn get_number(n: f64) -> JsValue;
    }

    let num = get_number(42.5);
    assert_eq!(num.as_f64(), Some(42.5));

    let num2 = get_number(-17.3);
    assert_eq!(num2.as_f64(), Some(-17.3));

    // Non-numbers should return None
    assert_eq!(JsValue::undefined().as_f64(), None);
    assert_eq!(JsValue::null().as_f64(), None);
    assert_eq!(JsValue::from_str("not a number").as_f64(), None);
}

pub(crate) fn test_jsvalue_arithmetic() {
    // Test arithmetic operators with JS numbers
    #[wasm_bindgen(inline_js = r#"
        export function get_num(n) { return n; }
        export function js_to_f64(v) { return +v; }
    "#)]
    extern "C" {
        fn get_num(n: f64) -> JsValue;
        fn js_to_f64(v: &JsValue) -> f64;
    }

    let a = get_num(10.0);
    let b = get_num(3.0);

    // Addition
    let result = a.add(&b);
    assert_eq!(js_to_f64(&result), 13.0);

    // Subtraction
    let result = a.sub(&b);
    assert_eq!(js_to_f64(&result), 7.0);

    // Multiplication
    let result = a.mul(&b);
    assert_eq!(js_to_f64(&result), 30.0);

    // Division
    let result = a.div(&b);
    assert!((js_to_f64(&result) - 3.333333).abs() < 0.001);

    // Checked division
    let result = a.checked_div(&b);
    assert!((js_to_f64(&result) - 3.333333).abs() < 0.001);

    // Remainder
    let result = a.rem(&b);
    assert_eq!(js_to_f64(&result), 1.0);

    // Power
    let result = get_num(2.0).pow(&get_num(3.0));
    assert_eq!(js_to_f64(&result), 8.0);

    // Negation
    let result = a.neg();
    assert_eq!(js_to_f64(&result), -10.0);
}

pub(crate) fn test_jsvalue_bitwise() {
    eprintln!("[TEST] Starting test_jsvalue_bitwise");
    // Test bitwise operators with JS numbers
    #[wasm_bindgen(inline_js = r#"
        export function make_num(n) { return n; }
        export function to_int(v) { return v | 0; }
    "#)]
    extern "C" {
        fn make_num(n: f64) -> JsValue;
        fn to_int(v: &JsValue) -> i32;
    }

    eprintln!("[TEST] Getting test values");
    let a = make_num(10.0); // 10
    let b = make_num(12.0); // 12

    eprintln!("[TEST] Testing bitwise AND");
    // Bitwise AND
    let result = a.bit_and(&b);
    eprintln!("[TEST] AND result obtained, converting to int");
    assert_eq!(to_int(&result), 8); // 0b1000

    // Bitwise OR
    let result = a.bit_or(&b);
    assert_eq!(to_int(&result), 14); // 0b1110

    // Bitwise XOR
    let result = a.bit_xor(&b);
    assert_eq!(to_int(&result), 6); // 0b0110

    // Bitwise NOT
    let result = a.bit_not();
    assert_eq!(to_int(&result), !10);

    // Left shift
    let result = make_num(5.0).shl(&make_num(2.0));
    assert_eq!(to_int(&result), 20); // 5 << 2 = 20

    // Signed right shift
    let result = make_num(20.0).shr(&make_num(2.0));
    assert_eq!(to_int(&result), 5); // 20 >> 2 = 5

    // Unsigned right shift
    let result = make_num(-1.0).unsigned_shr(&make_num(1.0));
    assert_eq!(result, 2147483647); // -1 >>> 1 = max positive i32
}

pub(crate) fn test_jsvalue_comparisons() {
    // Test comparison operators with JS numbers
    #[wasm_bindgen(inline_js = r#"
        export function get_val(n) { return n; }
    "#)]
    extern "C" {
        fn get_val(n: f64) -> JsValue;
    }

    let a = get_val(10.0);
    let b = get_val(20.0);
    let c = get_val(10.0);

    // Less than
    assert!(a.lt(&b));
    assert!(!b.lt(&a));
    assert!(!a.lt(&c));

    // Less than or equal
    assert!(a.le(&b));
    assert!(!b.le(&a));
    assert!(a.le(&c));

    // Greater than
    assert!(b.gt(&a));
    assert!(!a.gt(&b));
    assert!(!a.gt(&c));

    // Greater than or equal
    assert!(b.ge(&a));
    assert!(!a.ge(&b));
    assert!(a.ge(&c));

    // Loose equality (==)
    assert!(a.loose_eq(&c));
    assert!(!a.loose_eq(&b));
}

pub(crate) fn test_jsvalue_loose_eq_coercion() {
    // Test that loose_eq does type coercion like JS ==
    #[wasm_bindgen(inline_js = r#"
        export function get_num(n) { return n; }
        export function get_str(s) { return s; }
    "#)]
    extern "C" {
        fn get_num(n: f64) -> JsValue;
        fn get_str(s: &str) -> JsValue;
    }

    // Number 5 == string "5" in JS
    let num = get_num(5.0);
    let str_num = get_str("5");
    assert!(num.loose_eq(&str_num), "5 == '5' should be true in JS");

    // null == undefined in JS
    assert!(
        JsValue::null().loose_eq(&JsValue::undefined()),
        "null == undefined should be true in JS"
    );
}

pub(crate) fn test_jsvalue_js_in() {
    // Test the 'in' operator
    #[wasm_bindgen(inline_js = r#"
        export function get_obj() { return { foo: 42, bar: "hello" }; }
        export function get_prop(p) { return p; }
    "#)]
    extern "C" {
        fn get_obj() -> JsValue;
        fn get_prop(p: &str) -> JsValue;
    }

    let obj = get_obj();
    let foo_prop = get_prop("foo");
    let bar_prop = get_prop("bar");
    let baz_prop = get_prop("baz");

    // 'foo' in obj should be true
    assert!(foo_prop.js_in(&obj), "'foo' should be in object");

    // 'bar' in obj should be true
    assert!(bar_prop.js_in(&obj), "'bar' should be in object");

    // 'baz' in obj should be false
    assert!(!baz_prop.js_in(&obj), "'baz' should not be in object");
}

pub(crate) fn test_instanceof_basic() {
    // Test instanceof with built-in JS types
    #[wasm_bindgen(inline_js = r#"
        export function create_array() { return [1, 2, 3]; }
        export function create_date() { return new Date(); }
        export function create_error() { return new Error("test"); }
        export function create_object() { return { foo: "bar" }; }
        export function create_number() { return 42; }
        export function create_string() { return "hello"; }
    "#)]
    extern "C" {
        type Array;
        type Date;
        type Error;

        fn create_array() -> JsValue;
        fn create_date() -> JsValue;
        fn create_error() -> JsValue;
        fn create_object() -> JsValue;
        fn create_number() -> JsValue;
        fn create_string() -> JsValue;
    }

    // Array instanceof checks
    let arr = create_array();
    assert!(arr.has_type::<Array>(), "Array should be instanceof Array");
    assert!(!arr.has_type::<Date>(), "Array should not be instanceof Date");
    assert!(!arr.has_type::<Error>(), "Array should not be instanceof Error");

    // Date instanceof checks
    let date = create_date();
    assert!(date.has_type::<Date>(), "Date should be instanceof Date");
    assert!(!date.has_type::<Array>(), "Date should not be instanceof Array");

    // Error instanceof checks
    let error = create_error();
    assert!(error.has_type::<Error>(), "Error should be instanceof Error");
    assert!(!error.has_type::<Array>(), "Error should not be instanceof Array");

    // Plain object should not be instanceof Array/Date/Error
    let obj = create_object();
    assert!(!obj.has_type::<Array>(), "Object should not be instanceof Array");
    assert!(!obj.has_type::<Date>(), "Object should not be instanceof Date");
    assert!(!obj.has_type::<Error>(), "Object should not be instanceof Error");

    // Primitives should not be instanceof any class
    let num = create_number();
    assert!(!num.has_type::<Array>(), "Number should not be instanceof Array");

    let str_val = create_string();
    assert!(!str_val.has_type::<Array>(), "String should not be instanceof Array");
}

pub(crate) fn test_instanceof_is_instance_of() {
    use wasm_bindgen::JsCast;

    // Test is_instance_of method
    #[wasm_bindgen(inline_js = r#"
        export function make_array() { return []; }
        export function make_object() { return {}; }
    "#)]
    extern "C" {
        type Array;

        fn make_array() -> JsValue;
        fn make_object() -> JsValue;
    }

    let arr = make_array();
    let obj = make_object();

    // Test is_instance_of (same as has_type but different API)
    assert!(arr.is_instance_of::<Array>(), "Array is_instance_of Array");
    assert!(!obj.is_instance_of::<Array>(), "Object is not instance_of Array");
}

pub(crate) fn test_instanceof_dyn_into() {
    use wasm_bindgen::JsCast;

    // Test dyn_into for safe casting
    #[wasm_bindgen(inline_js = r#"
        export function get_array() { return [1, 2, 3]; }
        export function get_object() { return { x: 1 }; }
    "#)]
    extern "C" {
        #[derive(Debug)]
        type Array;

        fn get_array() -> JsValue;
        fn get_object() -> JsValue;
    }

    // dyn_into should succeed for correct type
    let arr_val = get_array();
    let arr_result: Result<Array, _> = arr_val.dyn_into();
    assert!(arr_result.is_ok(), "dyn_into should succeed for Array");

    // dyn_into should fail for wrong type
    let obj_val = get_object();
    let obj_result: Result<Array, _> = obj_val.dyn_into();
    assert!(obj_result.is_err(), "dyn_into should fail for non-Array");
}

pub(crate) fn test_instanceof_dyn_ref() {
    use wasm_bindgen::JsCast;

    // Test dyn_ref for safe reference casting
    #[wasm_bindgen(inline_js = r#"
        export function get_date() { return new Date(); }
        export function get_number() { return 123; }
    "#)]
    extern "C" {
        type Date;

        fn get_date() -> JsValue;
        fn get_number() -> JsValue;
    }

    // dyn_ref should return Some for correct type
    let date_val = get_date();
    let date_ref: Option<&Date> = date_val.dyn_ref();
    assert!(date_ref.is_some(), "dyn_ref should return Some for Date");

    // dyn_ref should return None for wrong type
    let num_val = get_number();
    let num_ref: Option<&Date> = num_val.dyn_ref();
    assert!(num_ref.is_none(), "dyn_ref should return None for non-Date");
}
