//! wry-bindgen - Runtime support for wasm-bindgen-style bindings over Wry's WebView
//!
//! This crate provides the runtime types and traits needed for the `#[wasm_bindgen]`
//! attribute macro to generate code that works with Wry's IPC protocol.
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`ipc`] - Binary IPC protocol types for message encoding/decoding
//! - [`encode`] - Core encoding/decoding traits for Rust types
//! - [`value`] - JsValue type representing JavaScript heap references
//! - [`function`] - JSFunction type for calling JavaScript functions
//! - [`batch`] - Batching system for grouping multiple JS operations
//! - [`runtime`] - Event loop and runtime management
//! - [`cast`] - Type casting trait for JavaScript types
//! - [`lazy`] - Lazy initialization for global JavaScript values

pub mod batch;
mod cast;
pub mod encode;
pub mod function;
pub mod ipc;
mod lazy;
pub mod runtime;
mod value;

// Re-export core types
pub use cast::JsCast;
pub use lazy::JsThreadLocal;
pub use value::JsValue;

// Re-export commonly used items
pub use batch::batch;
pub use encode::{BatchableResult, BinaryDecode, BinaryEncode, TypeConstructor};
pub use function::JSFunction;
pub use ipc::{DecodeError, DecodedData, DecodedVariant, EncodedData, IPCMessage, MessageType, decode_data};
pub use runtime::{WryRuntime, get_runtime, set_event_loop_proxy, wait_for_js_event};

// Re-export the macro
pub use wry_bindgen_macro::wasm_bindgen;

/// Macro to create a thread-local JavaScript value accessor.
///
/// This macro is used by the `#[wasm_bindgen(thread_local_v2)]` attribute
/// to generate lazy static accessors for JavaScript global values.
#[macro_export]
#[doc(hidden)]
macro_rules! __wry_bindgen_thread_local {
    ($actual_ty:ty = $value:expr) => {{
        std::thread_local! {
            pub static __INNER: $actual_ty = $value;
        }
        $crate::prelude::JsThreadLocal::new(&__INNER)
    }};
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::batch::batch;
    pub use crate::cast::JsCast;
    pub use crate::encode::{BatchableResult, BinaryDecode, BinaryEncode, TypeConstructor};
    pub use crate::function::JSFunction;
    pub use crate::lazy::JsThreadLocal;
    pub use crate::runtime::{set_event_loop_proxy, wait_for_js_event};
    pub use crate::value::JsValue;
    pub use crate::wasm_bindgen;
}
