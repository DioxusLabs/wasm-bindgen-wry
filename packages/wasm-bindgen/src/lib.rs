//! Unified wasm-bindgen shim crate
//!
//! This crate transparently re-exports either:
//! - wry-bindgen-core for desktop targets (non-wasm32)
//! - wasm-bindgen for wasm32 targets
//!
//! The `#[wasm_bindgen]` macro is a shim that expands to both implementations
//! wrapped in cfg-conditional modules.

#![no_std]
#![allow(hidden_glob_reexports)]

// Re-export the shim macro (works for both targets)
pub use wasm_bindgen_macro::__wasm_bindgen_class_marker;
pub use wasm_bindgen_macro::link_to;
pub use wasm_bindgen_macro::wasm_bindgen;

#[cfg(not(target_arch = "wasm32"))]
pub use wry_bindgen::*;

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen::*;

// Re-export the upstream wasm_bindgen macro for wasm32 targets
// This is used by the shim macro to delegate to the real wasm-bindgen
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen::prelude::wasm_bindgen as __wasm_bindgen_upstream_macro;

// Re-export the upstream class marker for wasm32 targets
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen::prelude::__wasm_bindgen_class_marker as __wasm_bindgen_upstream_class_marker;
