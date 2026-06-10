#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use js_sys_upstream::*;

#[cfg(any(not(target_arch = "wasm32"), feature = "unstable_force_wry_backend"))]
pub use js_sys_wry::*;

// Explicit imports take precedence over the backend glob, so the
// `wasm_bindgen` name consumers reach through `use js_sys::*` is the same
// crate as their own patched dependency; rustc accepts a glob/extern-prelude
// overlap only when both names resolve to the same crate. The `::` prefix
// resolves from the extern prelude rather than the glob-imported name.
pub use ::wasm_bindgen;
