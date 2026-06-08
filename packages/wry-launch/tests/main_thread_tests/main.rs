use std::process::ExitCode;

use wasm_bindgen::wasm_bindgen;

#[path = "../common/harness.rs"]
mod harness;

use harness::{BatchMode, TestCase, async_test, harness_main, sync_test, trial_name};

mod add_number_js;
#[allow(clippy::redundant_closure)]
mod async_bindings;
mod batch_stress;
mod borrow_stack;
mod callbacks;
mod catch_attribute;
mod clamped;
mod closure_panic;
mod closure_paths;
mod deferred_heap_refs;
mod export_call;
mod indexing;
mod is_type_of;
mod jsvalue;
mod module_import;
mod opaque_id_stress;
mod reentrant_callbacks;
mod roundtrip;
mod string_enum;
mod structs;
mod thread_local;
mod timer_callbacks;
mod upstream_attr_codegen;
mod wasm_bindgen_compat;

#[wasm_bindgen(inline_js = "export function heap_objects_alive(f) {
    return window.jsHeap.heapObjectsAlive();
}")]
extern "C" {
    /// Get the number of alive JS heap objects
    #[wasm_bindgen(js_name = heap_objects_alive)]
    pub fn heap_objects_alive() -> u32;
}

/// Generate one case per (test, batch_mode) pair for sync `fn()` tests.
macro_rules! sync_trials {
    ($tests:expr; $($module:ident :: $name:ident),* $(,)?) => {{
        $(
            for mode in [BatchMode::NonBatched, BatchMode::Batched] {
                $tests.push(sync_test(
                    trial_name(stringify!($module), stringify!($name), mode),
                    mode,
                    $module::$name,
                ));
            }
        )*
    }};
}

/// Generate one case per (test, batch_mode) pair for `async fn()` tests.
macro_rules! async_trials {
    ($tests:expr; $($module:ident :: $name:ident),* $(,)?) => {{
        $(
            for mode in [BatchMode::NonBatched, BatchMode::Batched] {
                $tests.push(async_test(
                    trial_name(stringify!($module), stringify!($name), mode),
                    mode,
                    $module::$name,
                ));
            }
        )*
    }};
}

fn build_tests() -> Vec<TestCase> {
    let mut tests: Vec<TestCase> = Vec::new();

    tests.push(sync_test(
        trial_name(
            "deferred_heap_refs",
            "test_nested_js_request_keeps_rust_deferred_heap_ref_frame",
            BatchMode::NonBatched,
        ),
        BatchMode::NonBatched,
        deferred_heap_refs::test_nested_js_request_keeps_rust_deferred_heap_ref_frame,
    ));

    tests.push(sync_test(
        trial_name(
            "deferred_heap_refs",
            "test_owned_deferred_heap_ref_can_be_used_before_drop",
            BatchMode::NonBatched,
        ),
        BatchMode::NonBatched,
        deferred_heap_refs::test_owned_deferred_heap_ref_can_be_used_before_drop,
    ));

    tests.push(async_test(
        trial_name(
            "opaque_id_stress",
            "test_opaque_id_double_free_stress",
            BatchMode::Batched,
        ),
        BatchMode::Batched,
        opaque_id_stress::test_opaque_id_double_free_stress,
    ));
    tests.push(async_test(
        trial_name(
            "batch_stress",
            "test_batch_stress_browser_event_callbacks",
            BatchMode::Batched,
        ),
        BatchMode::Batched,
        batch_stress::test_batch_stress_browser_event_callbacks,
    ));

    sync_trials!(tests;
        add_number_js::test_add_number_js,
        add_number_js::test_add_number_js_batch,
        roundtrip::test_roundtrip,
        callbacks::test_call_callback,
        callbacks::test_dropped_closure_disposes_js_callable,
        callbacks::test_dropped_once_closure_disposes_js_callable,
        callbacks::test_long_lived_callback_survives_setup_scope,
        callbacks::test_exported_method_drop_closure_disposes_js_callable,
        callbacks::test_mut_dyn_fn,
        callbacks::test_mut_dyn_fnmut,
        callbacks::test_batch_flushed_heap_ref_return_with_stack_callback,
        callbacks::test_js_callback_heap_ref_arg_with_pending_placeholders,
        callbacks::test_js_callback_multiple_heap_ref_args_share_request_id,
        callbacks::test_mut_dyn_fn_many_arity,
        callbacks::test_mut_dyn_fnmut_many_arity,
        closure_paths::test_explicit_dyn_wrapped_borrowed_event_callbacks,
        closure_paths::test_borrowed_first_once_callbacks,
        closure_paths::test_borrowed_first_rest_arg_callbacks,
        closure_paths::test_scoped_closure_borrow_constructors,
        closure_paths::test_callback_reference_and_constructor_variants,
        closure_paths::test_max_arity_closure_paths,
        reentrant_callbacks::test_reentrant_fn_closure,
        reentrant_callbacks::test_interleaved_fn_closures,
        closure_panic::test_closure_panic_surfaces_as_js_error,
        jsvalue::test_jsvalue_constants,
        jsvalue::test_jsvalue_bool,
        jsvalue::test_jsvalue_default,
        jsvalue::test_jsvalue_clone_reserved,
        jsvalue::test_jsvalue_equality,
        jsvalue::test_jsvalue_from_js,
        jsvalue::test_jsvalue_pass_to_js,
        jsvalue::test_jsvalue_as_string,
        jsvalue::test_jsvalue_as_f64,
        jsvalue::test_jsvalue_arithmetic,
        jsvalue::test_jsvalue_bigint_pow_preserves_bigint_semantics,
        jsvalue::test_jsvalue_bitwise,
        jsvalue::test_jsvalue_comparisons,
        jsvalue::test_jsvalue_loose_eq_coercion,
        jsvalue::test_jsvalue_js_in,
        jsvalue::test_instanceof_basic,
        jsvalue::test_instanceof_is_instance_of,
        jsvalue::test_instanceof_dyn_into,
        jsvalue::test_instanceof_dyn_ref,
        jsvalue::test_partial_eq_bool,
        jsvalue::test_partial_eq_numbers,
        jsvalue::test_partial_eq_strings,
        jsvalue::test_try_from_f64,
        jsvalue::test_try_from_string,
        jsvalue::test_owned_arithmetic_operators,
        jsvalue::test_owned_bitwise_operators,
        jsvalue::test_jscast_as_ref,
        jsvalue::test_as_ref_jsvalue,
        string_enum::test_string_enum_from_str,
        string_enum::test_string_enum_to_str,
        string_enum::test_string_enum_to_jsvalue,
        string_enum::test_string_enum_from_jsvalue,
        string_enum::test_string_enum_pass_to_js,
        string_enum::test_string_enum_receive_from_js,
        catch_attribute::test_catch_throws_error,
        catch_attribute::test_catch_successful_call,
        catch_attribute::test_catch_with_arguments,
        catch_attribute::test_catch_method,
        catch_attribute::test_result_alias_export_throws,
        structs::test_struct_bindings,
        structs::test_exported_struct_arg_before_heap_ref_arg,
        export_call::test_js_calls_exported_usize_js_thunk,
        export_call::test_js_calls_exported_usize_js_thunk_batched,
        export_call::test_unit_export_write_back_free_function,
        export_call::test_returning_export_write_back_order,
        export_call::test_unit_export_write_back_constructor,
        export_call::test_unit_export_write_back_static_method,
        export_call::test_unit_export_write_back_instance_method,
        export_call::test_unit_export_write_back_setter,
        clamped::test_clamped_is_uint8clampedarray,
        clamped::test_clamped_vec_is_uint8clampedarray,
        clamped::test_jsvalue_from_clamped_vec_is_uint8clampedarray,
        clamped::test_clamped_js_clamping_behavior,
        clamped::test_clamped_preserves_data,
        clamped::test_clamped_empty,
        clamped::test_clamped_mut_slice,
        borrow_stack::test_borrowed_ref_in_callback,
        borrow_stack::test_borrowed_ref_in_callback_with_return,
        borrow_stack::test_cloned_borrowed_ref_survives_callback,
        borrow_stack::test_wrapped_fn_event_ref_can_call_js_getter,
        borrow_stack::test_borrowed_ref_nested_frames,
        borrow_stack::test_borrowed_ref_deep_nesting,
        thread_local::test_thread_local,
        thread_local::test_thread_local_window,
        upstream_attr_codegen::test_variadic_import_spreads_final_argument,
        upstream_attr_codegen::test_imported_js_namespace_paths,
        upstream_attr_codegen::test_reexport_installs_imported_values,
        upstream_attr_codegen::test_static_string_thread_local_and_reexport,
        upstream_attr_codegen::test_namespaced_export_and_this,
        upstream_attr_codegen::test_start_export_runs_during_initialization,
        upstream_attr_codegen::test_numeric_enums_export_and_roundtrip,
        upstream_attr_codegen::test_dynamic_union_export_argument_decode,
        upstream_attr_codegen::test_dynamic_union_import_return_decode,
        upstream_attr_codegen::test_dynamic_union_nested_and_fallback,
        upstream_attr_codegen::test_dynamic_union_export_return_encode,
        upstream_attr_codegen::test_exported_class_metadata_paths,
        module_import::test_module_import,
        indexing::test_indexing_getter_array,
        indexing::test_indexing_setter_array,
        indexing::test_indexing_deleter_array,
        is_type_of::test_is_type_of_string,
        is_type_of::test_is_type_of_number,
        is_type_of::test_is_type_of_with_dyn_into,
        is_type_of::test_is_type_of_with_dyn_ref,
        is_type_of::test_has_type_with_is_type_of,
        wasm_bindgen_compat::test_imported_type_promising_compat,
        wasm_bindgen_compat::test_generic_import_erases_promise_method_shape,
        wasm_bindgen_compat::test_convert_traits_are_marker_bounds,
        wasm_bindgen_compat::test_interned_string_roundtrip,
        wasm_bindgen_compat::test_jsvalue_abi_ref_preserves_heap_ref,
        wasm_bindgen_compat::test_i64_try_from_bigint_preserves_precision_above_f64,
        wasm_bindgen_compat::test_u64_try_from_bigint_preserves_range,
        wasm_bindgen_compat::test_u128_try_from_bigint_preserves_range,
        wasm_bindgen_compat::test_i128_try_from_bigint_preserves_full_width,
        wasm_bindgen_compat::test_try_from_js_value_signed_numbers_preserve_negative_values,
    );

    async_trials!(tests;
        timer_callbacks::test_timer_callbacks,
        callbacks::test_call_callback_async,
        callbacks::test_join_many_callbacks_async,
        async_bindings::test_call_async,
        async_bindings::test_call_async_returning_js_value,
        async_bindings::test_catch_async_call_ok,
        async_bindings::test_catch_async_call_err,
        async_bindings::test_async_import_result_alias_propagates_err,
        async_bindings::test_async_export_result_alias_rejects,
        async_bindings::test_async_method,
        async_bindings::test_async_method_with_catch,
        async_bindings::test_async_static_method,
        async_bindings::test_already_resolved_async,
        async_bindings::test_already_rejected_async_catch,
        async_bindings::test_join_many_async,
        upstream_attr_codegen::test_async_export_returns_promise,
        upstream_attr_codegen::test_async_receiver_methods_return_promise,
        upstream_attr_codegen::test_async_constructor_returns_instance_promise,
        upstream_attr_codegen::test_async_static_method_returns_promise,
    );

    tests
}

fn main() -> ExitCode {
    harness_main(build_tests)
}
