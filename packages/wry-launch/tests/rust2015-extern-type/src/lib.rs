extern crate wasm_bindgen;

use wasm_bindgen::wasm_bindgen;

#[wasm_bindgen]
extern "C" {
    pub type Error;
}

pub fn touch() {}
