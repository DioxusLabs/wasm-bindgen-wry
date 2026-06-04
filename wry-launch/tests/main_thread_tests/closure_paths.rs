//! Coverage for closure conversion entrypoints that are easy to miss when
//! testing only `Closure::new` and simple `FnMut` callbacks.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::{Closure, JsValue, ScopedClosure, wasm_bindgen};
use web_sys::Event;
use wry_bindgen_runtime::force_flush;

pub(crate) fn test_explicit_dyn_wrapped_borrowed_event_callbacks() {
    #[wasm_bindgen(inline_js = r#"
        export function call_dyn_fn_event(cb) {
            cb(new Event("dyn-fn-event"));
        }

        export function call_dyn_fnmut_event(cb) {
            cb(new Event("dyn-fnmut-event"));
        }
    "#)]
    extern "C" {
        fn call_dyn_fn_event(cb: Closure<dyn Fn(&Event)>);
        fn call_dyn_fnmut_event(cb: Closure<dyn FnMut(&Event)>);
    }

    let seen_fn = Rc::new(RefCell::new(None));
    let seen_fn_for_callback = seen_fn.clone();
    let callback: Closure<dyn Fn(&Event)> = Closure::wrap(Box::new(move |event: &Event| {
        seen_fn_for_callback.borrow_mut().replace(event.type_());
    }) as Box<dyn Fn(&Event)>);

    call_dyn_fn_event(callback);
    force_flush();
    assert_eq!(seen_fn.borrow().as_deref(), Some("dyn-fn-event"));

    let seen_fnmut = Rc::new(RefCell::new(None));
    let call_count = Rc::new(Cell::new(0));
    let seen_fnmut_for_callback = seen_fnmut.clone();
    let call_count_for_callback = call_count.clone();
    let callback: Closure<dyn FnMut(&Event)> = Closure::wrap(Box::new(move |event: &Event| {
        call_count_for_callback.set(call_count_for_callback.get() + 1);
        seen_fnmut_for_callback.borrow_mut().replace(event.type_());
    }) as Box<dyn FnMut(&Event)>);

    call_dyn_fnmut_event(callback);
    force_flush();
    assert_eq!(seen_fnmut.borrow().as_deref(), Some("dyn-fnmut-event"));
    assert_eq!(call_count.get(), 1);
}

pub(crate) fn test_borrowed_first_once_callbacks() {
    #[wasm_bindgen(inline_js = r#"
        export function call_once_event(cb) {
            cb(new Event("once-event"));
        }

        export function call_once_event_and_number(cb) {
            return cb(new Event("once-rest-event"), 41);
        }
    "#)]
    extern "C" {
        fn call_once_event(cb: Closure<dyn FnMut(&Event)>);
        fn call_once_event_and_number(cb: Closure<dyn FnMut(&Event, u32) -> String>) -> String;
    }

    let seen_type = Rc::new(RefCell::new(None));
    let seen_type_for_callback = seen_type.clone();
    let callback = Closure::<dyn FnMut(&Event)>::once(move |event: &Event| {
        seen_type_for_callback.borrow_mut().replace(event.type_());
    });

    call_once_event(callback);
    force_flush();
    assert_eq!(seen_type.borrow().as_deref(), Some("once-event"));

    let callback =
        Closure::<dyn FnMut(&Event, u32) -> String>::once(move |event: &Event, value: u32| {
            format!("{}:{}", event.type_(), value + 1)
        });

    let result = call_once_event_and_number(callback);
    assert_eq!(result, "once-rest-event:42");
}

pub(crate) fn test_borrowed_first_rest_arg_callbacks() {
    #[wasm_bindgen(inline_js = r#"
        export function call_fnmut_event_and_number(cb) {
            return cb(new Event("fnmut-rest-event"), 40);
        }

        export function call_fn_event_and_object(cb) {
            return cb(new Event("fn-rest-event"), { label: "payload" });
        }

        export function read_payload_label(obj) {
            return obj.label;
        }
    "#)]
    extern "C" {
        fn call_fnmut_event_and_number(cb: Closure<dyn FnMut(&Event, u32) -> String>) -> String;
        fn call_fn_event_and_object(cb: Closure<dyn Fn(&Event, JsValue) -> String>) -> String;
        fn read_payload_label(obj: &JsValue) -> String;
    }

    let fnmut_calls = Rc::new(Cell::new(0));
    let fnmut_calls_for_callback = fnmut_calls.clone();
    let callback = Closure::new(move |event: &Event, value: u32| -> String {
        fnmut_calls_for_callback.set(fnmut_calls_for_callback.get() + 1);
        format!("{}:{}", event.type_(), value + 2)
    });

    let result = call_fnmut_event_and_number(callback);
    assert_eq!(result, "fnmut-rest-event:42");
    assert_eq!(fnmut_calls.get(), 1);

    let callback: Closure<dyn Fn(&Event, JsValue) -> String> =
        Closure::wrap(Box::new(move |event: &Event, payload: JsValue| {
            format!("{}:{}", event.type_(), read_payload_label(&payload))
        }));

    let result = call_fn_event_and_object(callback);
    assert_eq!(result, "fn-rest-event:payload");
}

pub(crate) fn test_scoped_closure_borrow_constructors() {
    #[wasm_bindgen(inline_js = r#"
        export function call_scoped_fn(cb, value) {
            return cb(value);
        }

        export function call_scoped_fnmut(cb, value) {
            return cb(value);
        }
    "#)]
    extern "C" {
        fn call_scoped_fn(cb: &ScopedClosure<'_, dyn Fn(u32) -> u32>, value: u32) -> u32;
        fn call_scoped_fnmut(cb: &ScopedClosure<'_, dyn FnMut(u32) -> u32>, value: u32) -> u32;
    }

    let add_one = |value| value + 1;
    let scoped = Closure::<dyn Fn(u32) -> u32>::borrow(&add_one);
    assert_eq!(call_scoped_fn(&scoped, 41), 42);

    let scoped = Closure::<dyn Fn(u32) -> u32>::borrow_aborting(&add_one);
    assert_eq!(call_scoped_fn(&scoped, 40), 41);

    let scoped = Closure::<dyn Fn(u32) -> u32>::borrow_assert_unwind_safe(&add_one);
    assert_eq!(call_scoped_fn(&scoped, 39), 40);

    let mut call_count = 0;
    let mut add_call_count = |value| {
        call_count += 1;
        value + call_count
    };

    let scoped = Closure::<dyn FnMut(u32) -> u32>::borrow_mut(&mut add_call_count);
    assert_eq!(call_scoped_fnmut(&scoped, 41), 42);
    drop(scoped);

    let scoped = Closure::<dyn FnMut(u32) -> u32>::borrow_mut_aborting(&mut add_call_count);
    assert_eq!(call_scoped_fnmut(&scoped, 40), 42);
    drop(scoped);

    let scoped =
        Closure::<dyn FnMut(u32) -> u32>::borrow_mut_assert_unwind_safe(&mut add_call_count);
    assert_eq!(call_scoped_fnmut(&scoped, 39), 42);
    drop(scoped);

    assert_eq!(call_count, 3);
}

pub(crate) fn test_callback_reference_and_constructor_variants() {
    #[wasm_bindgen(inline_js = r#"
        export function call_dyn_fn_ref(cb, value) {
            return cb(value);
        }

        export function call_owned_fnmut_callback(cb, value) {
            return cb(value);
        }

        export function call_owned_fn_callback(cb, value) {
            return cb(value);
        }

        export function call_js_value_callback(cb, value) {
            return cb(value);
        }
    "#)]
    extern "C" {
        fn call_dyn_fn_ref(cb: &dyn Fn(u32) -> u32, value: u32) -> u32;
        fn call_owned_fnmut_callback(cb: Closure<dyn FnMut(u32) -> u32>, value: u32) -> u32;
        fn call_owned_fn_callback(cb: Closure<dyn Fn(u32) -> u32>, value: u32) -> u32;
        fn call_js_value_callback(cb: &JsValue, value: u32) -> u32;
    }

    let add_two = |value| value + 2;
    assert_eq!(call_dyn_fn_ref(&add_two, 40), 42);

    let callback = Closure::<dyn FnMut(u32) -> u32>::own_aborting(|value| value + 1);
    assert_eq!(call_owned_fnmut_callback(callback, 41), 42);

    let callback = Closure::<dyn FnMut(u32) -> u32>::own_assert_unwind_safe(|value| value + 2);
    assert_eq!(call_owned_fnmut_callback(callback, 40), 42);

    let callback: Closure<dyn Fn(u32) -> u32> =
        Closure::wrap_aborting(Box::new(|value: u32| value + 3) as Box<dyn Fn(u32) -> u32>);
    assert_eq!(call_owned_fn_callback(callback, 39), 42);

    let callback: Closure<dyn Fn(u32) -> u32> = Closure::wrap_assert_unwind_safe(Box::new(
        |value: u32| value + 4,
    )
        as Box<dyn Fn(u32) -> u32>);
    assert_eq!(call_owned_fn_callback(callback, 38), 42);

    let callback = Closure::<dyn FnMut(u32) -> u32>::once_aborting(|value| value + 5);
    assert_eq!(call_owned_fnmut_callback(callback, 37), 42);

    let callback = Closure::<dyn FnMut(u32) -> u32>::once_assert_unwind_safe(|value| value + 6);
    assert_eq!(call_owned_fnmut_callback(callback, 36), 42);

    let callback = Closure::<dyn FnMut(u32) -> u32>::once_into_js(|value| value + 7);
    assert_eq!(call_js_value_callback(&callback, 35), 42);
}

pub(crate) fn test_max_arity_closure_paths() {
    #[wasm_bindgen(inline_js = r#"
        export function call_owned_arity8(cb) {
            return cb(1, 2, 3, 4, 5, 6, 7, 8);
        }

        export function call_borrowed_arity8(cb) {
            return cb(new Event("arity8-event"), 1, 2, 3, 4, 5, 6, 7);
        }

        export function call_fn_ref_arity7(cb) {
            return cb(1, 2, 3, 4, 5, 6, 7);
        }

        export function call_fnmut_ref_arity7(cb) {
            return cb(1, 2, 3, 4, 5, 6, 7);
        }
    "#)]
    extern "C" {
        fn call_owned_arity8(
            cb: Closure<dyn FnMut(u32, u32, u32, u32, u32, u32, u32, u32) -> u32>,
        ) -> u32;
        fn call_borrowed_arity8(
            cb: Closure<dyn FnMut(&Event, u32, u32, u32, u32, u32, u32, u32) -> String>,
        ) -> String;
        fn call_fn_ref_arity7(cb: &dyn Fn(u32, u32, u32, u32, u32, u32, u32) -> u32) -> u32;
        fn call_fnmut_ref_arity7(
            cb: &mut dyn FnMut(u32, u32, u32, u32, u32, u32, u32) -> u32,
        ) -> u32;
    }

    let callback = Closure::new(|a, b, c, d, e, f, g, h| a + b + c + d + e + f + g + h);
    assert_eq!(call_owned_arity8(callback), 36);

    let callback = Closure::new(|event: &Event, a, b, c, d, e, f, g| -> String {
        format!("{}:{}", event.type_(), a + b + c + d + e + f + g)
    });
    assert_eq!(call_borrowed_arity8(callback), "arity8-event:28");

    let sum7 = |a, b, c, d, e, f, g| a + b + c + d + e + f + g;
    assert_eq!(call_fn_ref_arity7(&sum7), 28);

    let mut call_count = 0;
    let mut sum7_mut = |a, b, c, d, e, f, g| {
        call_count += 1;
        a + b + c + d + e + f + g + call_count
    };
    assert_eq!(call_fnmut_ref_arity7(&mut sum7_mut), 29);
    assert_eq!(call_count, 1);
}
