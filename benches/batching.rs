use wasm_bindgen::wasm_bindgen;
use wry_testing;

#[wasm_bindgen(inline_js = "export function add(a, b) { return a + b; }")]
extern "C" {
    #[wasm_bindgen(js_name = add)]
    fn add_numbers(a: u32, b: u32) -> u32;
}

pub fn bench_batch_add_1() {
    add_numbers(10, 15);
}

pub fn bench_batch_add_100() {
    let _results =
        wry_testing::batch(|| (0..100).map(|_| add_numbers(10, 15)).collect::<Vec<u32>>());
}

#[wasm_bindgen(inline_js = "export function create_element(tag) {
        return document.createElement(tag);
    }")]
extern "C" {
    #[wasm_bindgen(js_name = create_element)]
    fn create_element(tag: &str) -> wry_testing::Element;
}

pub fn bench_batch_create_element_1() {
    let _elem = create_element("div");
}

pub fn bench_batch_create_element_100() {
    let _results = wry_testing::batch(|| {
        let tag = format!("div");
        (0..100)
            .map(|_| create_element(&tag))
            .collect::<Vec<wry_testing::Element>>()
    });
}
