use wasm_bindgen::wasm_bindgen;
use wry_launch::batch;

#[wasm_bindgen(inline_js = r#"
let thunkCalls = 0;

export function reset_thunk_calls() {
    thunkCalls = 0;
}

export function js_thunk() {
    thunkCalls += 1;
}

export function thunk_calls() {
    return thunkCalls;
}

export function call_exported_js_thunk_benchmark(n) {
    return window.JsThunkExportFixture.callJsThunkNTimes(n);
}

export function call_exported_js_thunk_benchmark_batched(n) {
    return window.JsThunkExportFixture.callJsThunkNTimesBatched(n);
}
"#)]
extern "C" {
    fn reset_thunk_calls();

    #[wasm_bindgen(js_name = js_thunk)]
    fn js_thunk();

    fn thunk_calls() -> u32;

    fn call_exported_js_thunk_benchmark(n: u32) -> u32;

    fn call_exported_js_thunk_benchmark_batched(n: u32) -> u32;
}

#[wasm_bindgen]
pub struct JsThunkExportFixture;

#[wasm_bindgen]
impl JsThunkExportFixture {
    #[wasm_bindgen(js_name = callJsThunkNTimes)]
    pub fn call_js_thunk_n_times(n: usize) -> u32 {
        for _ in 0..n {
            js_thunk();
        }
        n as u32
    }

    #[wasm_bindgen(js_name = callJsThunkNTimesBatched)]
    pub fn call_js_thunk_n_times_batched(n: usize) -> u32 {
        batch(|| {
            for _ in 0..n {
                js_thunk();
            }
        });
        n as u32
    }
}

pub(crate) fn test_js_calls_exported_usize_js_thunk() {
    reset_thunk_calls();

    let result = call_exported_js_thunk_benchmark(3);

    assert_eq!(result, 3);
    assert_eq!(thunk_calls(), 3);
}

pub(crate) fn test_js_calls_exported_usize_js_thunk_batched() {
    reset_thunk_calls();

    let result = call_exported_js_thunk_benchmark_batched(3);

    assert_eq!(result, 3);
    assert_eq!(thunk_calls(), 3);
}
