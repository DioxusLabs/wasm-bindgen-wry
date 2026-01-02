use wasm_bindgen::{Closure, wasm_bindgen};
use wry_testing::{bindings::set_on_error, set_on_log};

mod add_number_js;
mod borrow_stack;
mod callbacks;
mod catch_attribute;
mod clamped;
mod jsvalue;
mod module_import;
mod roundtrip;
mod string_enum;
mod thread_local;
mod structs;
mod async_bindings;

#[wasm_bindgen(inline_js = "export function heap_objects_alive(f) {
    return window.jsHeap.heapObjectsAlive();
}")]
extern "C" {
    /// Get the number of alive JS heap objects
    #[wasm_bindgen(js_name = heap_objects_alive)]
    pub fn heap_objects_alive() -> u32;
}

fn test_with_js_context_allow_new_js_values<F: FnOnce()>(f: F) {
    println!("testing {}", std::any::type_name::<F>());
    f();
}

fn test_with_js_context<F: FnOnce()>(f: F) {
    test_with_js_context_allow_new_js_values(|| {
        let before = heap_objects_alive();
        f();
        let after = heap_objects_alive();
        assert_eq!(before, after, "JS heap object leak detected");
    });
}

fn main() {
    wry_testing::run_headless(|| {
        set_on_error(Closure::new(|err: String, stack: String| {
            panic!("[ERROR IN JS CONSOLE] {}\nStack trace:\n{}", err, stack);
        }));

        set_on_log(Closure::new(|msg: String| {
            println!("[JS] {}", msg);
        }));

        // Adding numbers with and without batching
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

        // instanceof tests
        test_with_js_context(jsvalue::test_instanceof_basic);
        test_with_js_context(jsvalue::test_instanceof_is_instance_of);
        test_with_js_context(jsvalue::test_instanceof_dyn_into);
        test_with_js_context(jsvalue::test_instanceof_dyn_ref);

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

        // Struct bindings tests
        test_with_js_context(structs::test_struct_bindings);

        // Clamped type tests
        test_with_js_context(clamped::test_clamped_is_uint8clampedarray);
        test_with_js_context(clamped::test_clamped_vec_is_uint8clampedarray);
        test_with_js_context(clamped::test_clamped_js_clamping_behavior);
        test_with_js_context(clamped::test_clamped_preserves_data);
        test_with_js_context(clamped::test_clamped_empty);
        test_with_js_context(clamped::test_clamped_mut_slice);

        // Borrow stack tests
        test_with_js_context(borrow_stack::test_borrowed_ref_in_callback);
        test_with_js_context(borrow_stack::test_borrowed_ref_in_callback_with_return);
        // test_with_js_context(borrow_stack::test_borrowed_ref_nested_frames);
        // test_with_js_context(borrow_stack::test_borrowed_ref_deep_nesting);

        // Thread local tests
        test_with_js_context(thread_local::test_thread_local);
        test_with_js_context_allow_new_js_values(thread_local::test_thread_local_window);

        // Module import test
        test_with_js_context(module_import::test_module_import);

        // async bindings test
        test_with_js_context(async_bindings::test_call_async);
        test_with_js_context(async_bindings::test_call_async_returning_js_value);
        test_with_js_context(async_bindings::test_catch_async_call);
    })
    .unwrap();
}
