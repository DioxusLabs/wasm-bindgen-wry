use futures_util::{StreamExt, stream::futures_unordered};
use std::cell::Cell;
use wasm_bindgen::{Closure, wasm_bindgen};
use wry_bindgen_runtime::{batch, force_flush};
use wry_launch::JsValue;

pub(crate) fn test_call_callback() {
    #[wasm_bindgen(inline_js = "export function calls_callback(cb, value) { return cb(value); }")]
    extern "C" {
        #[wasm_bindgen(js_name = calls_callback)]
        fn calls_callback(cb: Closure<dyn FnMut(u32) -> u32>, value: u32) -> u32;
    }

    let callback = Closure::new(Box::new(|x: u32| x + 1) as Box<dyn FnMut(u32) -> u32>);
    let result = calls_callback(callback, 10);
    assert_eq!(result, 11);
}

pub(crate) async fn test_call_callback_async() {
    #[wasm_bindgen(
        inline_js = "export function calls_callback_async(cb, value) { setTimeout(() => { cb(value); }, 1); }"
    )]
    extern "C" {
        #[wasm_bindgen(js_name = calls_callback_async)]
        fn calls_callback_async(cb: Closure<dyn FnMut(u32)>, value: u32);
    }

    let (mut result_tx, mut result_rx) = futures_channel::mpsc::unbounded();
    let callback = Closure::new(move |x: u32| {
        println!("Callback called with value: {x}");
        result_tx.start_send(x + 1).unwrap();
    });
    println!("Calling calls_callback_async");
    let random = rand::random::<u32>() % 1000;
    calls_callback_async(callback, random);
    let result = result_rx.next().await.unwrap();
    assert_eq!(result, random + 1);
}

pub(crate) fn test_dropped_closure_disposes_js_callable() {
    #[wasm_bindgen(inline_js = r#"
        let storedDroppedCallback = null;

        export function store_callback_for_drop_test(cb) {
            storedDroppedCallback = cb;
        }

        export function dropped_callback_throws_after_rust_drop() {
            try {
                storedDroppedCallback(1);
                return false;
            } catch (error) {
                return String(error && error.message ? error.message : error)
                    .includes("already been dropped");
            }
        }

        export function clear_callback_for_drop_test() {
            storedDroppedCallback = null;
        }
    "#)]
    extern "C" {
        fn store_callback_for_drop_test(cb: &Closure<dyn FnMut(u32)>);
        fn dropped_callback_throws_after_rust_drop() -> bool;
        fn clear_callback_for_drop_test();
    }

    let called = std::rc::Rc::new(Cell::new(false));
    let called_clone = called.clone();
    let callback = Closure::new(move |_| {
        called_clone.set(true);
    });

    store_callback_for_drop_test(&callback);
    drop(callback);

    assert!(
        dropped_callback_throws_after_rust_drop(),
        "dropped closure should dispose the JS callable"
    );
    assert!(
        !called.get(),
        "dropped closure should not call back into Rust"
    );

    clear_callback_for_drop_test();
}

pub(crate) fn test_dropped_once_closure_disposes_js_callable() {
    #[wasm_bindgen(inline_js = r#"
        let storedDroppedOnceCallback = null;

        export function store_once_callback_for_drop_test(cb) {
            storedDroppedOnceCallback = cb;
        }

        export function dropped_once_callback_throws_after_rust_drop() {
            try {
                storedDroppedOnceCallback(1);
                return false;
            } catch (error) {
                return String(error && error.message ? error.message : error)
                    .includes("already been dropped");
            }
        }

        export function clear_once_callback_for_drop_test() {
            storedDroppedOnceCallback = null;
        }
    "#)]
    extern "C" {
        fn store_once_callback_for_drop_test(cb: &Closure<dyn FnMut(u32)>);
        fn dropped_once_callback_throws_after_rust_drop() -> bool;
        fn clear_once_callback_for_drop_test();
    }

    let called = std::rc::Rc::new(Cell::new(false));
    let called_clone = called.clone();
    let callback = Closure::<dyn FnMut(u32)>::once(move |_| {
        called_clone.set(true);
    });

    store_once_callback_for_drop_test(&callback);
    drop(callback);

    assert!(
        dropped_once_callback_throws_after_rust_drop(),
        "dropped once closure should dispose the JS callable"
    );
    assert!(
        !called.get(),
        "dropped once closure should not call back into Rust"
    );

    clear_once_callback_for_drop_test();
}

pub(crate) fn test_long_lived_callback_survives_setup_scope() {
    #[wasm_bindgen(inline_js = r#"
        let storedLongLivedCallback = null;

        export function store_long_lived_callback(cb) {
            storedLongLivedCallback = cb;
        }

        export function call_long_lived_callback(value) {
            try {
                return storedLongLivedCallback(value);
            } catch (_error) {
                return -1;
            }
        }

        export function clear_long_lived_callback() {
            storedLongLivedCallback = null;
        }
    "#)]
    extern "C" {
        fn store_long_lived_callback(cb: &Closure<dyn Fn(i32) -> i32>);
        fn call_long_lived_callback(value: i32) -> i32;
        fn clear_long_lived_callback();
    }

    {
        let callback: Closure<dyn Fn(i32) -> i32> = Closure::wrap(Box::new(|value| value + 1));
        store_long_lived_callback(&callback);
        callback.forget();
    }

    assert_eq!(call_long_lived_callback(41), 42);

    clear_long_lived_callback();
}

#[wasm_bindgen(inline_js = r#"
    let storedRuntimeBorrowedDropCallback = null;

    export function store_callback_for_runtime_borrowed_drop_test(cb) {
        storedRuntimeBorrowedDropCallback = cb;
    }

    export function drop_callback_from_exported_method(fixture) {
        fixture.dropCallback();
    }

    export function runtime_borrowed_dropped_callback_throws() {
        try {
            storedRuntimeBorrowedDropCallback(1);
            return false;
        } catch (error) {
            return String(error && error.message ? error.message : error)
                .includes("already been dropped");
        }
    }

    export function clear_runtime_borrowed_drop_callback() {
        storedRuntimeBorrowedDropCallback = null;
    }
"#)]
extern "C" {
    fn store_callback_for_runtime_borrowed_drop_test(cb: &Closure<dyn FnMut(u32)>);
    fn drop_callback_from_exported_method(fixture: &JsValue);
    fn runtime_borrowed_dropped_callback_throws() -> bool;
    fn clear_runtime_borrowed_drop_callback();
}

#[wasm_bindgen]
pub struct RuntimeBorrowedDropFixture {
    callback: Option<Closure<dyn FnMut(u32)>>,
}

#[wasm_bindgen]
impl RuntimeBorrowedDropFixture {
    #[wasm_bindgen(constructor)]
    pub fn new(_tag: u32) -> Self {
        let callback = Closure::new(|_| {});
        store_callback_for_runtime_borrowed_drop_test(&callback);
        Self {
            callback: Some(callback),
        }
    }

    #[wasm_bindgen(js_name = dropCallback)]
    pub fn drop_callback(&mut self) {
        drop(self.callback.take());
    }
}

pub(crate) fn test_exported_method_drop_closure_disposes_js_callable() {
    let fixture = JsValue::from(RuntimeBorrowedDropFixture::new(0));

    drop_callback_from_exported_method(&fixture);
    force_flush();

    assert!(
        runtime_borrowed_dropped_callback_throws(),
        "closure dropped inside exported method should dispose the JS callable"
    );

    clear_runtime_borrowed_drop_callback();
}

pub(crate) async fn test_join_many_callbacks_async() {
    #[wasm_bindgen(inline_js = "export async function identity(callback, key) {
        setTimeout(() => {
            callback(key);
        }, 10 + key % 10);
    }")]
    extern "C" {
        #[wasm_bindgen]
        fn identity(callback: Closure<dyn FnMut(JsValue)>, key: u32);
    }

    let mut futures = futures_unordered::FuturesUnordered::new();
    let mut expected = Vec::new();
    for i in 0..100u32 {
        let (tx, rx) = futures_channel::oneshot::channel();
        let closure = Closure::once(move |x: JsValue| {
            tx.send(x).unwrap();
        });
        identity(closure, i);
        futures.push(rx);
        expected.push(i);
    }
    while let Some(Ok(result)) = futures.next().await {
        let Some(index) = expected.iter().position(|&x| x == result) else {
            println!("Unexpected result: {result:?}");
            std::future::pending::<()>().await;
            break;
        };
        expected.remove(index);
    }
    assert!(
        expected.is_empty(),
        "Not all expected results were received"
    );
}

// Tests for &mut dyn Fn parameters
pub(crate) fn test_mut_dyn_fn() {
    #[wasm_bindgen(inline_js = r#"
        export function call_mut_dyn_fn(cb) { cb(); }
        export function call_mut_dyn_fn_with_arg(cb, value) { return cb(value); }
    "#)]
    extern "C" {
        fn call_mut_dyn_fn(cb: &mut dyn Fn());
        fn call_mut_dyn_fn_with_arg(cb: &mut dyn Fn(u32) -> u32, value: u32) -> u32;
    }

    // Test &mut dyn Fn() - Fn doesn't require mutability, but &mut reference should work
    let called = Cell::new(false);
    call_mut_dyn_fn(&mut || called.set(true));
    assert!(called.get(), "&mut dyn Fn() was not called");

    // Test &mut dyn Fn(u32) -> u32
    let result = call_mut_dyn_fn_with_arg(&mut |x| x + 1, 10);
    assert_eq!(result, 11, "&mut dyn Fn(u32) -> u32 returned wrong value");
}

// Tests for &mut dyn FnMut parameters
pub(crate) fn test_mut_dyn_fnmut() {
    #[wasm_bindgen(inline_js = r#"
        export function call_mut_dyn_fnmut(cb) { cb(); }
        export function call_mut_dyn_fnmut_with_arg(cb, value) { return cb(value); }
    "#)]
    extern "C" {
        fn call_mut_dyn_fnmut(cb: &mut dyn FnMut());
        fn call_mut_dyn_fnmut_with_arg(cb: &mut dyn FnMut(u32) -> u32, value: u32) -> u32;
    }

    // Test &mut dyn FnMut() with actual mutation
    let mut called = false;
    call_mut_dyn_fnmut(&mut || called = true);
    assert!(called, "&mut dyn FnMut() was not called");

    // Test &mut dyn FnMut(u32) -> u32 with mutation
    let mut call_count = 0;
    let result = call_mut_dyn_fnmut_with_arg(
        &mut |x| {
            call_count += 1;
            x + call_count
        },
        10,
    );
    assert_eq!(
        result, 11,
        "&mut dyn FnMut(u32) -> u32 returned wrong value"
    );
    assert_eq!(call_count, 1, "FnMut was not called exactly once");
}

pub(crate) fn test_batch_flushed_heap_ref_return_with_stack_callback() {
    #[wasm_bindgen(inline_js = r#"
        export function make_heap_ref(label) {
            return { label };
        }

        export function call_stack_callback_and_return_heap_ref(cb, value) {
            return { called: cb(value), value };
        }

        export function read_u32_field(obj, field) {
            return obj[field];
        }
    "#)]
    extern "C" {
        fn make_heap_ref(label: u32) -> JsValue;
        fn call_stack_callback_and_return_heap_ref(
            cb: &mut dyn FnMut(u32) -> u32,
            value: u32,
        ) -> JsValue;
        fn read_u32_field(obj: &JsValue, field: &str) -> u32;
    }

    let pending = make_heap_ref(7);
    let mut call_count = 0;
    let returned = call_stack_callback_and_return_heap_ref(
        &mut |value| {
            call_count += 1;
            value + 1
        },
        41,
    );

    assert_eq!(call_count, 1, "stack callback was not called exactly once");
    assert_eq!(read_u32_field(&pending, "label"), 7);
    assert_eq!(read_u32_field(&returned, "called"), 42);
    assert_eq!(read_u32_field(&returned, "value"), 41);
}

pub(crate) fn test_js_callback_heap_ref_arg_with_pending_placeholders() {
    #[wasm_bindgen(inline_js = r#"
        export function make_heap_ref_for_install_test(label) {
            return { label };
        }

        export function call_callback_with_heap_ref_arg(cb, label) {
            cb({ label });
        }

        export function read_heap_ref_label(obj) {
            return obj.label;
        }
    "#)]
    extern "C" {
        fn make_heap_ref_for_install_test(label: u32) -> JsValue;
        fn call_callback_with_heap_ref_arg(cb: &Closure<dyn FnMut(JsValue)>, label: u32);
        fn read_heap_ref_label(obj: &JsValue) -> u32;
    }

    let called = std::rc::Rc::new(Cell::new(false));
    let called_clone = called.clone();
    let callback = Closure::new(move |obj: JsValue| {
        assert_eq!(read_heap_ref_label(&obj), 99);
        called_clone.set(true);
    });

    batch(|| {
        let pending = make_heap_ref_for_install_test(7);
        call_callback_with_heap_ref_arg(&callback, 99);
        assert_eq!(read_heap_ref_label(&pending), 7);
    });

    assert!(called.get(), "callback was not called");
}

pub(crate) fn test_js_callback_multiple_heap_ref_args_share_request_id() {
    #[wasm_bindgen(inline_js = r#"
        export function call_callback_with_two_heap_ref_args(cb) {
            cb({ label: 11 }, { label: 22 });
        }

        export function read_multi_heap_ref_label(obj) {
            return obj.label;
        }
    "#)]
    extern "C" {
        fn call_callback_with_two_heap_ref_args(cb: &Closure<dyn FnMut(JsValue, JsValue)>);
        fn read_multi_heap_ref_label(obj: &JsValue) -> u32;
    }

    let called = std::rc::Rc::new(Cell::new(false));
    let called_clone = called.clone();
    let callback = Closure::new(move |first: JsValue, second: JsValue| {
        assert_eq!(read_multi_heap_ref_label(&first), 11);
        assert_eq!(read_multi_heap_ref_label(&second), 22);
        called_clone.set(true);
    });

    call_callback_with_two_heap_ref_args(&callback);

    assert!(called.get(), "callback was not called");
}

// Tests for &mut dyn Fn with multiple arities
pub(crate) fn test_mut_dyn_fn_many_arity() {
    #[wasm_bindgen(inline_js = r#"
        export function call_fn_arity0(cb) { cb(); }
        export function call_fn_arity1(cb) { cb(1); }
        export function call_fn_arity2(cb) { cb(1, 2); }
        export function call_fn_arity3(cb) { cb(1, 2, 3); }
    "#)]
    extern "C" {
        fn call_fn_arity0(cb: &mut dyn Fn());
        fn call_fn_arity1(cb: &mut dyn Fn(u32));
        fn call_fn_arity2(cb: &mut dyn Fn(u32, u32));
        fn call_fn_arity3(cb: &mut dyn Fn(u32, u32, u32));
    }

    let called = Cell::new(false);
    call_fn_arity0(&mut || called.set(true));
    assert!(called.get());

    let called = Cell::new(false);
    call_fn_arity1(&mut |a| {
        assert_eq!(a, 1);
        called.set(true);
    });
    assert!(called.get());

    let called = Cell::new(false);
    call_fn_arity2(&mut |a, b| {
        assert_eq!((a, b), (1, 2));
        called.set(true);
    });
    assert!(called.get());

    let called = Cell::new(false);
    call_fn_arity3(&mut |a, b, c| {
        assert_eq!((a, b, c), (1, 2, 3));
        called.set(true);
    });
    assert!(called.get());
}

// Tests for &mut dyn FnMut with multiple arities
pub(crate) fn test_mut_dyn_fnmut_many_arity() {
    #[wasm_bindgen(inline_js = r#"
        export function call_fnmut_arity0(cb) { cb(); }
        export function call_fnmut_arity1(cb) { cb(1); }
        export function call_fnmut_arity2(cb) { cb(1, 2); }
        export function call_fnmut_arity3(cb) { cb(1, 2, 3); }
    "#)]
    extern "C" {
        fn call_fnmut_arity0(cb: &mut dyn FnMut());
        fn call_fnmut_arity1(cb: &mut dyn FnMut(u32));
        fn call_fnmut_arity2(cb: &mut dyn FnMut(u32, u32));
        fn call_fnmut_arity3(cb: &mut dyn FnMut(u32, u32, u32));
    }

    let mut called = false;
    call_fnmut_arity0(&mut || called = true);
    assert!(called);

    let mut called = false;
    call_fnmut_arity1(&mut |a| {
        assert_eq!(a, 1);
        called = true;
    });
    assert!(called);

    let mut called = false;
    call_fnmut_arity2(&mut |a, b| {
        assert_eq!((a, b), (1, 2));
        called = true;
    });
    assert!(called);

    let mut called = false;
    call_fnmut_arity3(&mut |a, b, c| {
        assert_eq!((a, b, c), (1, 2, 3));
        called = true;
    });
    assert!(called);
}
