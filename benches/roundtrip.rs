use wasm_bindgen::wasm_bindgen;

#[wasm_bindgen(inline_js = "
    export function identity_u32(x) { return x; }
    export function identity_u64(x) { return x; }
    export function identity_i32(x) { return x; }
    export function identity_i64(x) { return x; }
    export function identity_f32(x) { return x; }
    export function identity_f64(x) { return x; }
    export function identity_bool(x) { return x; }
    export function identity_string(x) { return x; }
    export function identity_option(x) { return x; }
")]
extern "C" {
    #[wasm_bindgen(js_name = identity_u32)]
    fn identity_u32(x: u32) -> u32;

    #[wasm_bindgen(js_name = identity_u64)]
    fn identity_u64(x: u64) -> u64;

    #[wasm_bindgen(js_name = identity_i32)]
    fn identity_i32(x: i32) -> i32;

    #[wasm_bindgen(js_name = identity_i64)]
    fn identity_i64(x: i64) -> i64;

    #[wasm_bindgen(js_name = identity_f32)]
    fn identity_f32(x: f32) -> f32;

    #[wasm_bindgen(js_name = identity_f64)]
    fn identity_f64(x: f64) -> f64;

    #[wasm_bindgen(js_name = identity_bool)]
    fn identity_bool(x: bool) -> bool;

    #[wasm_bindgen(js_name = identity_string)]
    fn identity_string(x: String) -> String;

    #[wasm_bindgen(js_name = identity_option)]
    fn identity_option(x: Option<u32>) -> Option<u32>;
}

pub fn bench_roundtrip_u32() {
    let _ = identity_u32(42);
}

pub fn bench_roundtrip_u64() {
    let _ = identity_u64(42);
}

pub fn bench_roundtrip_i32() {
    let _ = identity_i32(-42);
}

pub fn bench_roundtrip_i64() {
    let _ = identity_i64(-42);
}

pub fn bench_roundtrip_f32() {
    let _ = identity_f32(3.14159);
}

pub fn bench_roundtrip_f64() {
    let _ = identity_f64(3.14159);
}

pub fn bench_roundtrip_bool() {
    let _ = identity_bool(true);
}

pub fn bench_roundtrip_string() {
    let _ = identity_string("Hello, world!".to_string());
}

pub fn bench_roundtrip_large_string() {
    let _ = identity_string("Hello, world!".repeat(100));
}

pub fn bench_roundtrip_option_some() {
    let _ = identity_option(Some(42));
}

pub fn bench_roundtrip_option_none() {
    let _ = identity_option(None);
}
