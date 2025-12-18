use wasm_bindgen::wasm_bindgen;
use wry_testing::set_on_log;

mod add_number_js;
mod callbacks;
mod catch_attribute;
mod jsvalue;
mod string_enum;
mod roundtrip;

#[wasm_bindgen(inline_js = "export function heap_objects_alive(f) {
        return window.jsHeap.heapObjectsAlive();
    }")]
extern "C" {
    /// Get the number of alive JS heap objects
    #[wasm_bindgen(js_name = heap_objects_alive)]
    pub fn heap_objects_alive() -> u32;
}

fn test_with_js_context<F: FnOnce()>(f: F) {
    println!("testing {}", std::any::type_name::<F>());
    let before = heap_objects_alive();
    f();
    let after = heap_objects_alive();
    assert_eq!(before, after, "JS heap object leak detected");
}

fn main() {
    wry_testing::run(|| {
        set_on_log(Box::new(|msg: String| {
            println!("[JS] {}", msg);
        }));

        // The simplest bindings
        test_with_js_context(add_number_js::test_add_number_js);
        test_with_js_context(add_number_js::test_add_number_js_batch);

        // Roundtrip tests
        test_with_js_context(roundtrip::test_roundtrip);

        // Callbacks
        test_with_js_context(callbacks::test_call_callback);
        test_with_js_context(callbacks::test_call_callback_async);

        // JsValue behavior tests
        test_with_js_context(jsvalue::test_jsvalue_constants);
        test_with_js_context(jsvalue::test_jsvalue_bool);
        test_with_js_context(jsvalue::test_jsvalue_default);
        test_with_js_context(jsvalue::test_jsvalue_clone_reserved);
        test_with_js_context(jsvalue::test_jsvalue_equality);
        test_with_js_context(jsvalue::test_jsvalue_from_js);
        test_with_js_context(jsvalue::test_jsvalue_pass_to_js);
        test_with_js_context(jsvalue::test_jsvalue_as_string);
        test_with_js_context(jsvalue::test_jsvalue_as_f64);
        test_with_js_context(jsvalue::test_jsvalue_arithmetic);
        test_with_js_context(jsvalue::test_jsvalue_bitwise);
        test_with_js_context(jsvalue::test_jsvalue_comparisons);
        test_with_js_context(jsvalue::test_jsvalue_loose_eq_coercion);
        test_with_js_context(jsvalue::test_jsvalue_js_in);

        // String enum tests
        test_with_js_context(string_enum::test_string_enum_from_str);
        test_with_js_context(string_enum::test_string_enum_to_str);
        test_with_js_context(string_enum::test_string_enum_to_jsvalue);
        test_with_js_context(string_enum::test_string_enum_from_jsvalue);

        // Catch attribute tests
        test_with_js_context(catch_attribute::test_catch_throws_error);
        test_with_js_context(catch_attribute::test_catch_successful_call);
        test_with_js_context(catch_attribute::test_catch_with_arguments);
        test_with_js_context(catch_attribute::test_catch_method);
    })
    .unwrap();
}
