use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::{Closure, wasm_bindgen};

/// Test that dyn Fn closures can be called reentrantly.
/// This creates a closure that calls itself via JS, which should work
/// for Fn closures but would panic for FnMut closures.
pub(crate) fn test_reentrant_fn_closure() {
    #[wasm_bindgen(inline_js = r#"
        let storedCallback = null;
        export function store_callback(cb) {
            storedCallback = cb;
        }
        export function call_stored_callback(depth) {
            return storedCallback(depth);
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = store_callback)]
        fn store_callback(cb: &Closure<dyn Fn(u32) -> u32>);

        #[wasm_bindgen(js_name = call_stored_callback)]
        fn call_stored_callback(depth: u32) -> u32;
    }

    let call_count = Rc::new(Cell::new(0u32));
    let call_count_clone = call_count.clone();

    // Create a Fn closure that calls itself via JS when depth > 0
    // Use Closure::wrap with explicit Box<dyn Fn> to ensure Fn (not FnMut)
    let callback: Closure<dyn Fn(u32) -> u32> = Closure::wrap(Box::new(move |depth: u32| {
        call_count_clone.set(call_count_clone.get() + 1);

        if depth > 0 {
            // Reentrant call: this closure calling itself via JS
            call_stored_callback(depth - 1) + 1
        } else {
            1
        }
    }) as Box<dyn Fn(u32) -> u32>);

    store_callback(&callback);

    // Call with depth 3: should result in 4 calls (depth 3, 2, 1, 0)
    let result = call_stored_callback(3);

    assert_eq!(result, 4, "Expected sum of 4 from reentrant calls");
    assert_eq!(call_count.get(), 4, "Expected 4 total calls");

    println!("Reentrant Fn closure test passed: {} calls, result = {}", call_count.get(), result);
}

/// Test that multiple different Fn closures can interleave calls.
pub(crate) fn test_interleaved_fn_closures() {
    #[wasm_bindgen(inline_js = r#"
        let callbackA = null;
        let callbackB = null;
        export function store_callback_a(cb) { callbackA = cb; }
        export function store_callback_b(cb) { callbackB = cb; }
        export function call_a(n) { return callbackA(n); }
        export function call_b(n) { return callbackB(n); }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = store_callback_a)]
        fn store_callback_a(cb: &Closure<dyn Fn(u32) -> u32>);
        #[wasm_bindgen(js_name = store_callback_b)]
        fn store_callback_b(cb: &Closure<dyn Fn(u32) -> u32>);
        #[wasm_bindgen(js_name = call_a)]
        fn call_a(n: u32) -> u32;
        #[wasm_bindgen(js_name = call_b)]
        fn call_b(n: u32) -> u32;
    }

    let total_calls = Rc::new(Cell::new(0u32));

    let total_a = total_calls.clone();
    let callback_a: Closure<dyn Fn(u32) -> u32> = Closure::wrap(Box::new(move |n: u32| {
        total_a.set(total_a.get() + 1);
        if n > 0 {
            // A calls B
            call_b(n - 1) + 10
        } else {
            1
        }
    }) as Box<dyn Fn(u32) -> u32>);

    let total_b = total_calls.clone();
    let callback_b: Closure<dyn Fn(u32) -> u32> = Closure::wrap(Box::new(move |n: u32| {
        total_b.set(total_b.get() + 1);
        if n > 0 {
            // B calls A
            call_a(n - 1) + 100
        } else {
            2
        }
    }) as Box<dyn Fn(u32) -> u32>);

    store_callback_a(&callback_a);
    store_callback_b(&callback_b);

    // Call A(2): A(2) -> B(1) -> A(0) -> returns 1
    // Result: 1 + 10 (from A(2)) + 100 (from B(1)) = 111... wait let me trace:
    // call_a(2) = call_b(1) + 10 = (call_a(0) + 100) + 10 = (1 + 100) + 10 = 111
    let result = call_a(2);

    assert_eq!(result, 111, "Expected 111 from interleaved calls");
    assert_eq!(total_calls.get(), 3, "Expected 3 total calls");

    println!("Interleaved Fn closures test passed: {} calls, result = {}", total_calls.get(), result);
}
