use wasm_bindgen::{Clamped, wasm_bindgen};

/// Test that Clamped<&[u8]> is received as Uint8ClampedArray in JS
pub(crate) fn test_clamped_is_uint8clampedarray() {
    #[wasm_bindgen(inline_js = "export function is_uint8_clamped_array(arr) {
        return arr instanceof Uint8ClampedArray;
    }")]
    extern "C" {
        fn is_uint8_clamped_array(arr: Clamped<&[u8]>) -> bool;
    }

    let data: &[u8] = &[0, 128, 255];
    assert!(
        is_uint8_clamped_array(Clamped(data)),
        "Clamped<&[u8]> should be received as Uint8ClampedArray in JS"
    );
}

/// Test that Clamped<Vec<u8>> is received as Uint8ClampedArray in JS
pub(crate) fn test_clamped_vec_is_uint8clampedarray() {
    #[wasm_bindgen(inline_js = "export function is_uint8_clamped_array_vec(arr) {
        return arr instanceof Uint8ClampedArray;
    }")]
    extern "C" {
        fn is_uint8_clamped_array_vec(arr: Clamped<Vec<u8>>) -> bool;
    }

    let data = vec![0u8, 128u8, 255u8];
    assert!(
        is_uint8_clamped_array_vec(Clamped(data)),
        "Clamped<Vec<u8>> should be received as Uint8ClampedArray in JS"
    );
}

/// Test that Clamped values clamp correctly when set (JS behavior)
pub(crate) fn test_clamped_js_clamping_behavior() {
    #[wasm_bindgen(inline_js = "export function test_clamping() {
        // Create a Uint8ClampedArray and test clamping behavior
        const arr = new Uint8ClampedArray(3);
        arr[0] = -10;   // Should clamp to 0
        arr[1] = 300;   // Should clamp to 255
        arr[2] = 128;   // Should stay 128
        return [arr[0], arr[1], arr[2]];
    }")]
    extern "C" {
        fn test_clamping() -> Vec<u8>;
    }

    let result = test_clamping();
    assert_eq!(result, vec![0, 255, 128], "Uint8ClampedArray should clamp values to 0-255 range");
}

/// Test sending and receiving Clamped data preserves values
pub(crate) fn test_clamped_preserves_data() {
    #[wasm_bindgen(inline_js = "export function sum_clamped(arr) {
        let sum = 0;
        for (let i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        return sum;
    }")]
    extern "C" {
        fn sum_clamped(arr: Clamped<&[u8]>) -> u32;
    }

    let data: &[u8] = &[10, 20, 30, 40, 50];
    let sum = sum_clamped(Clamped(data));
    assert_eq!(sum, 150, "Sum of Clamped array should be correct");
}

/// Test empty Clamped array
pub(crate) fn test_clamped_empty() {
    #[wasm_bindgen(inline_js = "export function clamped_length(arr) {
        return arr.length;
    }")]
    extern "C" {
        fn clamped_length(arr: Clamped<&[u8]>) -> u32;
    }

    let data: &[u8] = &[];
    let len = clamped_length(Clamped(data));
    assert_eq!(len, 0, "Empty Clamped array should have length 0");
}

/// Test Clamped with mutable slice
pub(crate) fn test_clamped_mut_slice() {
    #[wasm_bindgen(inline_js = "export function is_clamped_mut(arr) {
        return arr instanceof Uint8ClampedArray;
    }")]
    extern "C" {
        fn is_clamped_mut(arr: Clamped<&mut [u8]>) -> bool;
    }

    let mut data = [1u8, 2u8, 3u8];
    assert!(
        is_clamped_mut(Clamped(&mut data)),
        "Clamped<&mut [u8]> should be received as Uint8ClampedArray in JS"
    );
}
