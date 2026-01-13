use futures_util::{StreamExt, stream::futures_unordered};
use std::cell::Cell;
use wasm_bindgen::{Closure, wasm_bindgen};
use wry_testing::JsValue;

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
