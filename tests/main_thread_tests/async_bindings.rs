use pollster::FutureExt;
use wasm_bindgen::wasm_bindgen;
use wry_testing::JsValue;

pub(crate) fn test_call_async() {
    #[wasm_bindgen(inline_js = "export async function set_value_after_1_second(a, b) {
        return new Promise((resolve) => {
            setTimeout(() => {
                window.value_after_1_second = a + b;
                resolve()
            }, 100);
        });
    }
    export function get_value_after_1_second() {
        return window.value_after_1_second;
    }")]
    extern "C" {
        #[wasm_bindgen]
        async fn set_value_after_1_second(a: u32, b: u32);
        #[wasm_bindgen]
        fn get_value_after_1_second() -> u32;
    }

    let future = set_value_after_1_second(2, 3);
    println!("Waiting for async function to complete...");
    let _: () = future.block_on();
    println!("Async function completed.");
    let result = get_value_after_1_second();
    assert_eq!(result, 5);
}

pub(crate) fn test_call_async_returning_js_value() {
    #[wasm_bindgen(inline_js = "export async function add_async(a, b) {
        return new Promise((resolve) => {
            setTimeout(() => {
                resolve(a + b)
            }, 100);
        });
    }")]
    extern "C" {
        #[wasm_bindgen]
        async fn add_async(a: u32, b: u32) -> JsValue;
    }

    let result = add_async(2, 3).block_on();
    assert_eq!(result.as_f64().unwrap() as u32, 5);
}


pub(crate) fn test_catch_async_call() {
    #[wasm_bindgen(inline_js = "export async function throw_value(error) {
        return new Promise((resolve, reject) => {
            setTimeout(() => {
                reject(error);
            }, 100);
        });
    }
    export async function identity(value) {
        return new Promise((resolve) => {
            setTimeout(() => {
                resolve(value);
            }, 100);
        });
    }")]
    extern "C" {
        #[wasm_bindgen(catch)]
        async fn throw_value(error: &str) -> Result<(), JsValue>;
        #[wasm_bindgen(catch)]
        async fn identity(value: &str) -> Result<JsValue, JsValue>;
    }

    let result = throw_value("Test error").block_on();
    let err = result.err().unwrap();
    assert_eq!(err.as_string().unwrap(), "Test error");

    let result = identity("Hello, world!").block_on().unwrap();
    assert_eq!(result.as_string().unwrap(), "Hello, world!");
}


