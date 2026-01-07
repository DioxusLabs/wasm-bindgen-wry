use wasm_bindgen::wasm_bindgen;

pub(crate) fn test_add_number_js() {
    #[wasm_bindgen(inline_js = "export function add(a, b) { return a + b; }")]
    extern "C" {
        #[wasm_bindgen(js_name = add)]
        fn add_numbers(a: u32, b: u32) -> u32;
    }

    let result = add_numbers(2, 3);
    assert_eq!(result, 5);
}

pub(crate) fn test_add_number_js_batch() {
    #[wasm_bindgen(inline_js = "export function add(a, b) { return a + b; }")]
    extern "C" {
        #[wasm_bindgen(js_name = add)]
        fn add_numbers(a: u32, b: u32) -> u32;
    }

    let results =
        wry_testing::batch(|| (0..100).map(|_| add_numbers(10, 15)).collect::<Vec<u32>>());
    for result in results {
        assert_eq!(result, 25);
    }
}
