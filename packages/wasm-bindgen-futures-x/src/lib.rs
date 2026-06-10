#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use wasm_bindgen_futures_upstream::*;

#[cfg(any(not(target_arch = "wasm32"), feature = "unstable_force_wry_backend"))]
pub use wasm_bindgen_futures_wry::*;
