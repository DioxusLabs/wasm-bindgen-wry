use pollster::FutureExt;
use wasm_bindgen::wasm_bindgen;

pub(crate) fn test_call_async() {
    #[wasm_bindgen(inline_js = "export async function set_value_after_1_second(a, b) {
        return new Promise((resolve) => {
            setTimeout(() => {
                window.value_after_1_second = a + b;
                resolve()
            }, 1000);
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

    set_value_after_1_second(2, 3).block_on();
    let result = get_value_after_1_second();
    assert_eq!(result, 5);
}
