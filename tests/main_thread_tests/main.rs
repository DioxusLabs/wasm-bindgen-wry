use tokio::select;
use wasm_bindgen::wasm_bindgen;

mod add_number_js;
mod async_bindings;
mod borrow_stack;
mod callbacks;
mod catch_attribute;
mod clamped;
mod indexing;
mod is_type_of;
mod jsvalue;
mod module_import;
mod reentrant_callbacks;
mod roundtrip;
mod run_on_main_thread;
mod string_enum;
mod structs;
mod thread_local;

#[wasm_bindgen(inline_js = "export function heap_objects_alive(f) {
    return window.jsHeap.heapObjectsAlive();
}")]
extern "C" {
    /// Get the number of alive JS heap objects
    #[wasm_bindgen(js_name = heap_objects_alive)]
    pub fn heap_objects_alive() -> u32;
}

async fn test_with_js_context_allow_new_js_values<F: FnOnce()>(f: F) {
    async_test_with_js_context_allow_new_js_values(async || f()).await;
}

async fn async_test_with_js_context_allow_new_js_values<
    Fut: std::future::Future<Output = ()>,
    F: FnOnce() -> Fut,
>(
    f: F,
) {
    println!("testing {}", std::any::type_name::<F>());
    select! {
        result = f() => result,
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
            panic!("Test timed out after 5 seconds");
        }
    };
}

async fn test_with_js_context<F: FnOnce()>(f: F) {
    async_test_with_js_context_allow_new_js_values(async || f()).await;
}

async fn async_test_with_js_context<Fut: std::future::Future<Output = ()>, F: FnOnce() -> Fut>(
    f: F,
) {
    async_test_with_js_context_allow_new_js_values(|| async move {
        // let before = heap_objects_alive();
        f().await;
        // let after = heap_objects_alive();
        // assert_eq!(before, after, "JS heap object leak detected");
    })
    .await;
}

fn main() {
    wry_testing::run_headless(|| async {
        // Adding numbers with and without batching
        test_with_js_context(add_number_js::test_add_number_js).await;
        test_with_js_context(add_number_js::test_add_number_js_batch).await;

        // Roundtrip tests
        test_with_js_context(roundtrip::test_roundtrip).await;

        // Callbacks
        test_with_js_context(callbacks::test_call_callback).await;
        async_test_with_js_context(callbacks::test_call_callback_async).await;
        async_test_with_js_context(callbacks::test_join_many_callbacks_async).await;

        // Reentrant callbacks (dyn Fn)
        test_with_js_context(reentrant_callbacks::test_reentrant_fn_closure).await;
        test_with_js_context(reentrant_callbacks::test_interleaved_fn_closures).await;

        // JsValue behavior tests
        test_with_js_context(jsvalue::test_jsvalue_constants).await;
        test_with_js_context(jsvalue::test_jsvalue_bool).await;
        test_with_js_context(jsvalue::test_jsvalue_default).await;
        test_with_js_context(jsvalue::test_jsvalue_clone_reserved).await;
        test_with_js_context(jsvalue::test_jsvalue_equality).await;
        test_with_js_context(jsvalue::test_jsvalue_from_js).await;
        test_with_js_context(jsvalue::test_jsvalue_pass_to_js).await;
        test_with_js_context(jsvalue::test_jsvalue_as_string).await;
        test_with_js_context(jsvalue::test_jsvalue_as_f64).await;
        test_with_js_context(jsvalue::test_jsvalue_arithmetic).await;
        test_with_js_context(jsvalue::test_jsvalue_bitwise).await;
        test_with_js_context(jsvalue::test_jsvalue_comparisons).await;
        test_with_js_context(jsvalue::test_jsvalue_loose_eq_coercion).await;
        test_with_js_context(jsvalue::test_jsvalue_js_in).await;

        // instanceof tests
        test_with_js_context(jsvalue::test_instanceof_basic).await;
        test_with_js_context(jsvalue::test_instanceof_is_instance_of).await;
        test_with_js_context(jsvalue::test_instanceof_dyn_into).await;
        test_with_js_context(jsvalue::test_instanceof_dyn_ref).await;

        // Stable API additions tests
        test_with_js_context(jsvalue::test_partial_eq_bool).await;
        test_with_js_context(jsvalue::test_partial_eq_numbers).await;
        test_with_js_context(jsvalue::test_partial_eq_strings).await;
        test_with_js_context(jsvalue::test_try_from_f64).await;
        test_with_js_context(jsvalue::test_try_from_string).await;
        test_with_js_context(jsvalue::test_owned_arithmetic_operators).await;
        test_with_js_context(jsvalue::test_owned_bitwise_operators).await;
        test_with_js_context(jsvalue::test_jscast_as_ref).await;
        test_with_js_context(jsvalue::test_as_ref_jsvalue).await;

        // String enum tests
        test_with_js_context(string_enum::test_string_enum_from_str).await;
        test_with_js_context(string_enum::test_string_enum_to_str).await;
        test_with_js_context(string_enum::test_string_enum_to_jsvalue).await;
        test_with_js_context(string_enum::test_string_enum_from_jsvalue).await;
        test_with_js_context(string_enum::test_string_enum_pass_to_js).await;
        test_with_js_context(string_enum::test_string_enum_receive_from_js).await;

        // Catch attribute tests
        test_with_js_context(catch_attribute::test_catch_throws_error).await;
        test_with_js_context(catch_attribute::test_catch_successful_call).await;
        test_with_js_context(catch_attribute::test_catch_with_arguments).await;
        test_with_js_context(catch_attribute::test_catch_method).await;

        // Struct bindings tests
        test_with_js_context(structs::test_struct_bindings).await;

        // Clamped type tests
        test_with_js_context(clamped::test_clamped_is_uint8clampedarray).await;
        test_with_js_context(clamped::test_clamped_vec_is_uint8clampedarray).await;
        test_with_js_context(clamped::test_clamped_js_clamping_behavior).await;
        test_with_js_context(clamped::test_clamped_preserves_data).await;
        test_with_js_context(clamped::test_clamped_empty).await;
        test_with_js_context(clamped::test_clamped_mut_slice).await;

        // Borrow stack tests
        test_with_js_context(borrow_stack::test_borrowed_ref_in_callback).await;
        test_with_js_context(borrow_stack::test_borrowed_ref_in_callback_with_return).await;
        test_with_js_context(borrow_stack::test_borrowed_ref_nested_frames).await;
        test_with_js_context(borrow_stack::test_borrowed_ref_deep_nesting).await;

        // Thread local tests
        test_with_js_context(thread_local::test_thread_local).await;
        test_with_js_context_allow_new_js_values(thread_local::test_thread_local_window).await;

        // Module import test
        test_with_js_context(module_import::test_module_import).await;

        // Indexing tests
        test_with_js_context(indexing::test_indexing_getter_array).await;
        test_with_js_context(indexing::test_indexing_setter_array).await;
        test_with_js_context(indexing::test_indexing_deleter_array).await;

        // is_type_of tests
        test_with_js_context(is_type_of::test_is_type_of_string).await;
        test_with_js_context(is_type_of::test_is_type_of_number).await;
        test_with_js_context(is_type_of::test_is_type_of_with_dyn_into).await;
        test_with_js_context(is_type_of::test_is_type_of_with_dyn_ref).await;
        test_with_js_context(is_type_of::test_has_type_with_is_type_of).await;

        // async bindings test
        async_test_with_js_context(async_bindings::test_call_async).await;
        async_test_with_js_context(async_bindings::test_call_async_returning_js_value).await;
        async_test_with_js_context(async_bindings::test_catch_async_call_ok).await;
        async_test_with_js_context(async_bindings::test_catch_async_call_err).await;
        async_test_with_js_context(async_bindings::test_async_method).await;
        async_test_with_js_context(async_bindings::test_async_method_with_catch).await;
        async_test_with_js_context(async_bindings::test_async_static_method).await;
        async_test_with_js_context(async_bindings::test_join_many_async).await;

        // run_on_main_thread tests
        test_with_js_context(run_on_main_thread::test_run_on_main_thread_basic).await;
        test_with_js_context(run_on_main_thread::test_run_on_main_thread_verifies_thread).await;
    })
    .unwrap();
}
