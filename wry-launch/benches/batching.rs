use criterion::black_box;
use wasm_bindgen::wasm_bindgen;
use wry_launch::{self, JsValue};

#[wasm_bindgen(inline_js = "export function add(a, b) { return a + b; }")]
extern "C" {
    #[wasm_bindgen(js_name = add)]
    fn add_numbers(a: u32, b: u32) -> u32;
}

pub fn bench_batch_add_1() {
    black_box(add_numbers(black_box(10), black_box(15)));
}

pub fn bench_batch_add_100() {
    let results = wry_launch::batch(|| {
        (0..100)
            .map(|_| add_numbers(black_box(10), black_box(15)))
            .collect::<Vec<u32>>()
    });
    black_box(results);
}

#[wasm_bindgen(inline_js = "export function create_element(tag) {
        return document.createElement(tag);
    }")]
extern "C" {
    #[wasm_bindgen(js_name = create_element)]
    fn create_element(tag: &str) -> JsValue;
}

pub fn bench_batch_create_element_1() {
    black_box(create_element(black_box("div")));
}

pub fn bench_batch_create_element_100() {
    wry_launch::batch(|| {
        let tag = "div".to_string();
        let results = (0..100)
            .map(|_| create_element(black_box(&tag)))
            .collect::<Vec<_>>();
        black_box(results);
    });
}
