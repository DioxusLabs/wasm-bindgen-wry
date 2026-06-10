#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(target_arch = "wasm32")]
pub use web_sys_upstream::*;

#[cfg(not(target_arch = "wasm32"))]
pub use web_sys_wry::*;
