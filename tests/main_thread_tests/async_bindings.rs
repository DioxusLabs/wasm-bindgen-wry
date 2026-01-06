use std::time::Instant;

use futures_util::{StreamExt, stream::futures_unordered};
use wasm_bindgen::wasm_bindgen;
use wry_testing::JsValue;

pub(crate) async fn test_call_async() {
    #[wasm_bindgen(inline_js = "export async function set_value_after_1_second(a, b) {
        return new Promise((resolve) => {
            setTimeout(() => {
                window.value_after_1_second = a + b;
                resolve()
            }, 10);
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
    let start = Instant::now();
    let _: () = future.await;
    let duration = start.elapsed();
    assert!(
        duration.as_millis() >= 10,
        "Async function returned too quickly"
    );
    println!("Async function completed after {:?}", duration);
    let result = get_value_after_1_second();
    assert_eq!(result, 5);
}

pub(crate) async fn test_call_async_returning_js_value() {
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

    let result = add_async(2, 3).await;
    assert_eq!(result.as_f64().unwrap() as u32, 5);
}

pub(crate) async fn test_catch_async_call_ok() {
    #[wasm_bindgen(inline_js = "export async function identity(value) {
        return new Promise((resolve) => {
            setTimeout(() => {
                console.log('Resolving identity with value:', value);
                resolve(value);
            }, 100);
        });
    }")]
    extern "C" {
        #[wasm_bindgen(catch)]
        async fn identity(value: &str) -> Result<JsValue, JsValue>;
    }
    println!("Testing async function that returns a value...");
    let result = identity("Hello, world!").await.unwrap();
    assert_eq!(result.as_string().unwrap(), "Hello, world!");
}

pub(crate) async fn test_catch_async_call_err() {
    #[wasm_bindgen(inline_js = "export async function throw_value(error) {
        return new Promise((resolve, reject) => {
            setTimeout(() => {
                reject(error);
            }, 100);
        });
    }")]
    extern "C" {
        #[wasm_bindgen(catch)]
        async fn throw_value(error: &str) -> Result<(), JsValue>;
    }

    println!("Testing async function that throws an error...");
    let result = throw_value("Test error").await;
    let err = result.err().unwrap();
    assert_eq!(err.as_string().unwrap(), "Test error");
}

pub(crate) async fn test_async_method() {
    #[wasm_bindgen(inline_js = "export class AsyncCalculator {
        constructor(base) {
            this.base = base;
        }
        async add_after_delay(value) {
            return new Promise((resolve) => {
                setTimeout(() => {
                    resolve(this.base + value);
                }, 100);
            });
        }
    }")]
    extern "C" {
        type AsyncCalculator;

        #[wasm_bindgen(constructor)]
        fn new(base: u32) -> AsyncCalculator;

        #[wasm_bindgen(method)]
        async fn add_after_delay(this: &AsyncCalculator, value: u32) -> JsValue;
    }

    let calc = AsyncCalculator::new(10);
    let result = calc.add_after_delay(5).await;
    assert_eq!(result.as_f64().unwrap() as u32, 15);
}

pub(crate) async fn test_async_method_with_catch() {
    #[wasm_bindgen(inline_js = "export class AsyncValidator {
        constructor(shouldFail) {
            this.shouldFail = shouldFail;
        }
        async validate(value) {
            return new Promise((resolve, reject) => {
                setTimeout(() => {
                    if (this.shouldFail) {
                        reject('Validation failed: ' + value);
                    } else {
                        resolve('Valid: ' + value);
                    }
                }, 100);
            });
        }
    }")]
    extern "C" {
        type AsyncValidator;

        #[wasm_bindgen(constructor)]
        fn new(should_fail: bool) -> AsyncValidator;

        #[wasm_bindgen(method, catch)]
        async fn validate(this: &AsyncValidator, value: &str) -> Result<JsValue, JsValue>;
    }

    // Test successful validation
    println!("Testing async method with successful validation...");
    let validator_ok = AsyncValidator::new(false);
    println!("Validator created, calling validate method...");
    let result = validator_ok.validate("test").await.unwrap();
    assert_eq!(result.as_string().unwrap(), "Valid: test");
    println!("Successful validation passed.");

    // Test failed validation
    println!("Testing async method with failed validation...");
    let validator_fail = AsyncValidator::new(true);
    println!("Validator created, calling validate method...");
    let result = validator_fail.validate("test").await;
    let err = result.err().unwrap();
    assert_eq!(err.as_string().unwrap(), "Validation failed: test");
}

pub(crate) async fn test_async_static_method() {
    #[wasm_bindgen(inline_js = "export class AsyncUtils {
        static async fetch_data(key) {
            return new Promise((resolve) => {
                setTimeout(() => {
                    resolve('data_for_' + key);
                }, 100);
            });
        }
    }")]
    extern "C" {
        type AsyncUtils;

        #[wasm_bindgen(static_method_of = AsyncUtils)]
        async fn fetch_data(key: &str) -> JsValue;
    }

    let result = AsyncUtils::fetch_data("test_key").await;
    assert_eq!(result.as_string().unwrap(), "data_for_test_key");
}

pub(crate) async fn test_join_many_async() {
    #[wasm_bindgen(inline_js = "export async function identity(key) {
        return new Promise((resolve) => {
            setTimeout(() => {
                resolve(key);
            }, 100 + key);
        });
    }")]
    extern "C" {
        async fn identity(key: u64) -> JsValue;
    }

    let mut futures = futures_unordered::FuturesUnordered::new();
    let mut expected = Vec::new();
    for i in 0..100 {
        futures.push(identity(i));
        expected.push(i);
    }
    while let Some(result) = futures.next().await {
        println!("Got result: {:?}", result);
        let as_u64 = result.as_f64().unwrap() as u64;
        let index = expected.iter().position(|&x| x == as_u64).unwrap();
        expected.remove(index);
    }
}
