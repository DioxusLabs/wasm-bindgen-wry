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

// Use the wry backend on every non-wasm32 target, and on wasm32 too when
// `unstable_force_wry_backend` is enabled.
#[cfg(any(not(target_arch = "wasm32"), feature = "unstable_force_wry_backend"))]
pub use wry_bindgen::*;

#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use wasm_bindgen_upstream::*;

// Re-export the upstream wasm_bindgen macro for wasm32 targets
// This is used by the shim macro to delegate to the real wasm-bindgen
#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use wasm_bindgen_upstream::prelude::wasm_bindgen as __wasm_bindgen_upstream_macro;

// Re-export the upstream class marker for wasm32 targets
#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use wasm_bindgen_upstream::prelude::__wasm_bindgen_class_marker as __wasm_bindgen_upstream_class_marker;

// Re-export the upstream link_to macro for wasm32 targets
#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use wasm_bindgen_upstream::link_to as __wasm_bindgen_upstream_link_to;
