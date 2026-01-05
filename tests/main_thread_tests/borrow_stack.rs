//! Tests for the borrowed reference stack implementation.
//!
//! The borrow stack uses indices 1-127 for temporary borrowed references
//! that are automatically cleaned up after each operation completes.

use std::rc::Rc;

use wasm_bindgen::{JsValue, wasm_bindgen};

/// Test borrowed refs with callbacks
pub(crate) fn test_borrowed_ref_in_callback() {
    use wasm_bindgen::Closure;

    #[wasm_bindgen(inline_js = r#"
        export function call_with_value(cb, val) { return cb(val); }
        export function make_value() { return { x: 42 }; }
    "#)]
    extern "C" {
        fn call_with_value(cb: Closure<dyn FnMut(&JsValue)>, val: &JsValue);
        fn make_value() -> JsValue;
    }

    let val = make_value();

    let response = Rc::new(std::cell::Cell::new(false));
    let response_clone = response.clone();
    // Callback receives a borrowed ref
    let callback = Closure::new(move |v: &JsValue| {
        // The value should be an object with x = 42
        println!("calling is_undefined");
        let is_undefined = v.is_undefined();
        println!("is_undefined = {}", is_undefined);
        println!("calling is_null");
        let is_null = v.is_null();
        println!("is_null = {}", is_null);
        response_clone.set(!is_undefined && !is_null)
    });

    call_with_value(callback, &val);
    let result = response.get();
    assert!(result, "Callback should receive valid borrowed ref");
}

/// Test borrowed refs with callbacks
pub(crate) fn test_borrowed_ref_in_callback_with_return() {
    use wasm_bindgen::Closure;

    #[wasm_bindgen(inline_js = r#"
        export function call_with_value(cb, val) { return cb(val); }
        export function make_value() { return { x: 42 }; }
    "#)]
    extern "C" {
        fn call_with_value(cb: Closure<dyn FnMut(&JsValue) -> bool>, val: &JsValue) -> bool;
        fn make_value() -> JsValue;
    }

    let val = make_value();

    // Callback receives a borrowed ref
    let callback = Closure::new(move |v: &JsValue| {
        // The value should be an object with x = 42
        !v.is_undefined() && !v.is_null()
    });

    let result = call_with_value(callback, &val);
    assert!(result, "Callback should receive valid borrowed ref");
}

/// Test nested borrow stack frames: when we call a JS function that passes a reference
/// to a Rust closure which then calls another JS function with a reference,
/// the outer reference should still be valid after the inner call returns.
///
/// Call stack:
/// 1. Rust calls JS with borrowed ref to `outer_obj`
/// 2. JS calls Rust callback with borrowed ref to `inner_obj`
/// 3. Rust callback calls JS with borrowed ref to `innermost_obj`
/// 4. Inner calls return, but `outer_obj` ref should still be valid
pub(crate) fn test_borrowed_ref_nested_frames() {
    use std::cell::Cell;
    use wasm_bindgen::Closure;

    #[wasm_bindgen(inline_js = r#"
        export function call_with_refs(outer_ref, callback) {
            // outer_ref is the first borrowed ref
            const outer_value = outer_ref.name;

            // Call the Rust callback with another borrowed ref
            const inner_obj = { name: "inner" };
            const callback_result = callback(inner_obj);

            // After callback returns, check outer_ref is still valid
            const outer_still_valid = outer_ref.name === "outer";

            return { outer_value, callback_result, outer_still_valid };
        }

        export function check_ref(obj) {
            // This is called from inside the Rust callback with a third borrowed ref
            return obj.name === "innermost";
        }

        export function make_outer() { return { name: "outer" }; }
        export function make_innermost() { return { name: "innermost" }; }

        export function get_result_field(result, field) {
            return result[field];
        }
    "#)]
    extern "C" {
        fn call_with_refs(
            outer_ref: &JsValue,
            callback: Closure<dyn FnMut(&JsValue) -> bool>,
        ) -> JsValue;
        fn check_ref(obj: &JsValue) -> bool;
        fn make_outer() -> JsValue;
        fn make_innermost() -> JsValue;
        fn get_result_field(result: &JsValue, field: &str) -> JsValue;
    }

    let outer_obj = make_outer();
    let innermost_obj = make_innermost();

    // Track that the callback was actually called
    let callback_was_called = Rc::new(Cell::new(false));
    let innermost_check_passed = Rc::new(Cell::new(false));

    // Clone innermost_obj so we can use it inside the closure
    let innermost_for_closure = innermost_obj.clone();

    let callback = Closure::new({
        let callback_was_called = callback_was_called.clone();
        let innermost_check_passed = innermost_check_passed.clone();
        move |inner_ref: &JsValue| {
            callback_was_called.set(true);

            // Verify the inner_ref is valid
            assert!(
                !inner_ref.is_undefined(),
                "inner_ref should not be undefined"
            );
            assert!(!inner_ref.is_null(), "inner_ref should not be null");

            // Now call another JS function with yet another borrowed ref (innermost)
            // This creates a third level of nesting
            let check_result = check_ref(&innermost_for_closure);
            innermost_check_passed.set(check_result);

            check_result
        }
    });

    // Call JS with outer_obj as borrowed ref
    // JS will call our callback with inner_obj as borrowed ref
    // Our callback will call JS with innermost_obj as borrowed ref
    let result = call_with_refs(&outer_obj, callback);

    // Verify callback was called
    assert!(
        callback_was_called.get(),
        "Callback should have been called"
    );

    // Verify innermost check passed
    assert!(
        innermost_check_passed.get(),
        "Innermost ref check should have passed"
    );

    // Verify the result from JS
    let outer_value = get_result_field(&result, "outer_value");
    assert_eq!(
        outer_value.as_string(),
        Some("outer".to_string()),
        "outer_value should be 'outer'"
    );

    let callback_result = get_result_field(&result, "callback_result");
    assert_eq!(
        callback_result.as_bool(),
        Some(true),
        "callback_result should be true"
    );

    let outer_still_valid = get_result_field(&result, "outer_still_valid");
    assert_eq!(
        outer_still_valid.as_bool(),
        Some(true),
        "outer_ref should still be valid after inner callback returns"
    );
}

/// Test that deeply nested borrow frames work correctly
/// This tests 4 levels of nesting to stress test the frame management:
/// Rust -> JS(ref1) -> Rust callback -> JS(ref2) -> Rust callback -> JS(ref3) -> Rust callback -> JS(ref4)
/// Each level verifies that its reference remains valid after inner calls return.
pub(crate) fn test_borrowed_ref_deep_nesting() {
    use wasm_bindgen::Closure;

    #[wasm_bindgen(inline_js = r#"
        export function level1(ref1, cb1) {
            console.log("In level1");
            const v1 = ref1.level;
            const result2 = cb1({ level: 2 });
            const valid1 = ref1.level === 1;
            return { v1, result2, valid1 };
        }

        export function level2(ref2, cb2) {
            console.log("In level2", ref2, cb2);
            const v2 = ref2.level;
            const result3 = cb2({ level: 3 });
            const valid2 = ref2.level === 2;
            return { v2, result3, valid2 };
        }

        export function level3(ref3, cb3) {
            console.log("In level3");
            const v3 = ref3.level;
            const result4 = cb3({ level: 4 });
            const valid3 = ref3.level === 3;
            return { v3, result4, valid3 };
        }

        export function level4(ref4) {
            console.log("In level4");
            return { v4: ref4.level, valid4: ref4.level === 4 };
        }

        export function make_level1() { return { level: 1 }; }
        export function extract(obj, key) { return obj[key]; }
    "#)]
    extern "C" {
        fn level1(ref1: &JsValue, cb1: Closure<dyn FnMut(&JsValue) -> JsValue>) -> JsValue;
        fn level2(ref2: &JsValue, cb2: Closure<dyn FnMut(&JsValue) -> JsValue>) -> JsValue;
        fn level3(ref3: &JsValue, cb3: Closure<dyn FnMut(&JsValue) -> JsValue>) -> JsValue;
        fn level4(ref4: &JsValue) -> JsValue;
        fn make_level1() -> JsValue;
        fn extract(obj: &JsValue, key: &str) -> JsValue;
    }

    let obj1 = make_level1();

    // Create nested callbacks - each level calls the next
    let result = level1(&obj1, Closure::new(move |ref2: &JsValue| -> JsValue {
        let out = level2(
            ref2,
            Closure::new(move |ref3: &JsValue| -> JsValue {
                let out = level3(
                    ref3,
                    Closure::new(move |ref4: &JsValue| -> JsValue {
                        let out = level4(ref4);
                        out
                    }),
                );
                out
            }),
        );
        out
    }));

    // Verify all levels saw their correct values
    assert_eq!(
        extract(&result, "v1").as_f64(),
        Some(1.0),
        "Level 1 should see value 1"
    );
    assert_eq!(
        extract(&result, "valid1").as_bool(),
        Some(true),
        "Level 1 ref should remain valid"
    );

    let result2 = extract(&result, "result2");
    assert_eq!(
        extract(&result2, "v2").as_f64(),
        Some(2.0),
        "Level 2 should see value 2"
    );
    assert_eq!(
        extract(&result2, "valid2").as_bool(),
        Some(true),
        "Level 2 ref should remain valid"
    );

    let result3 = extract(&result2, "result3");
    assert_eq!(
        extract(&result3, "v3").as_f64(),
        Some(3.0),
        "Level 3 should see value 3"
    );
    assert_eq!(
        extract(&result3, "valid3").as_bool(),
        Some(true),
        "Level 3 ref should remain valid"
    );

    let result4 = extract(&result3, "result4");
    assert_eq!(
        extract(&result4, "v4").as_f64(),
        Some(4.0),
        "Level 4 should see value 4"
    );
    assert_eq!(
        extract(&result4, "valid4").as_bool(),
        Some(true),
        "Level 4 ref should be valid"
    );
}
