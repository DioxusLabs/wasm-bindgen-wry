#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_futures_upstream::*;

#[cfg(not(target_arch = "wasm32"))]
pub use wasm_bindgen_futures_wry::*;
