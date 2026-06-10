#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(target_arch = "wasm32", not(feature = "unstable_force_wry_backend")))]
pub use web_sys_upstream::*;

#[cfg(any(not(target_arch = "wasm32"), feature = "unstable_force_wry_backend"))]
pub use web_sys_wry::*;

// Explicit imports take precedence over the backend glob, so the `js_sys` and
// `wasm_bindgen` names consumers reach through `use web_sys::*` are the same
// crates as their own patched dependencies; rustc accepts a glob/extern-prelude
// overlap only when both names resolve to the same crate. The `::` prefix
// resolves from the extern prelude rather than the glob-imported names.
pub use ::js_sys;
pub use ::wasm_bindgen;
