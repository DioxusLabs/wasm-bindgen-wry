use wasm_bindgen::wasm_bindgen;

pub(crate) fn test_module_import() {
    #[wasm_bindgen(module = "./test_module.js")]
    extern "C" {
        fn multiply(a: u32, b: u32) -> u32;
    }

    let result = multiply(3, 4);
    assert_eq!(result, 12);
}
