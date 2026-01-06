use futures_util::StreamExt;
use wasm_bindgen::{Closure, wasm_bindgen};

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
    let callback = Closure::new(Box::new(move |x: u32| {
        println!("Callback called with value: {}", x);
        result_tx.start_send(x + 1).unwrap();
    }) as Box<dyn FnMut(u32)>);
    println!("Calling calls_callback_async");
    let random = rand::random::<u32>() % 1000;
    calls_callback_async(callback, random);
    let result = result_rx.next().await.unwrap();
    assert_eq!(result, random + 1);
}
