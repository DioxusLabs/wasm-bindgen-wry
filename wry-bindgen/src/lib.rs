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
mod js_helpers;
mod lazy;
#[cfg(feature = "runtime")]
pub mod runtime;
mod value;

/// Runtime module for wasm-bindgen compatibility.
/// This module provides stubs for wasm-specific functions that js-sys uses directly.
pub mod __rt {
    /// Stub for wasm memory casting - not supported in wry-bindgen.
    /// This function is only called for TypedArray::view() which requires wasm memory.
    #[inline]
    pub unsafe fn wbg_cast<From: ?Sized, To>(_val: &From) -> To {
        panic!("wbg_cast is not supported in wry-bindgen - TypedArray::view() requires wasm memory")
    }
}

/// Closure type for passing Rust closures to JavaScript.
/// Note: This is a stub implementation for API compatibility.
/// Actual closure passing is not yet fully supported in wry-bindgen.
pub struct Closure<T: ?Sized> {
    _marker: std::marker::PhantomData<T>,
    _value: JsValue,
}

impl<T: ?Sized> Closure<T> {
    /// Forgets the closure, leaking it.
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

impl<T: ?Sized> AsRef<JsValue> for Closure<T> {
    fn as_ref(&self) -> &JsValue {
        &self._value
    }
}

// Implement encoding traits for Closure
impl<T: ?Sized> encode::TypeConstructor for Closure<T> {
    fn create_type_instance() -> String {
        "new window.ClosureType()".to_string()
    }
}

impl<T: ?Sized> encode::BinaryEncode for Closure<T> {
    fn encode(self, encoder: &mut ipc::EncodedData) {
        self._value.encode(encoder);
    }
}

impl<T: ?Sized> encode::BinaryEncode for &Closure<T> {
    fn encode(self, encoder: &mut ipc::EncodedData) {
        (&self._value).encode(encoder);
    }
}

// Re-export core types
pub use cast::JsCast;
pub use lazy::JsThreadLocal;
pub use value::JsValue;

// Re-export commonly used items
pub use batch::batch;
pub use encode::{BatchableResult, BinaryDecode, BinaryEncode, TypeConstructor};
pub use function::JSFunction;
pub use ipc::{
    DecodeError, DecodedData, DecodedVariant, EncodedData, IPCMessage, MessageType, decode_data,
};
#[cfg(feature = "runtime")]
pub use runtime::{WryRuntime, get_runtime, set_event_loop_proxy, wait_for_js_result};

// Re-export the macro
pub use wry_bindgen_macro::wasm_bindgen;

// Re-export inventory for macro use

pub use inventory;

/// Function specification for the registry
#[derive(Clone, Copy)]
pub struct JsFunctionSpec {
    /// Function that generates the JS code
    pub js_code: fn() -> String,
    pub type_info: fn() -> (Vec<String>, String),
}

/// Inline JS module info
#[derive(Clone, Copy)]
pub struct InlineJsModule {
    /// The JS module content
    pub content: &'static str,
}

impl InlineJsModule {
    pub const fn new(content: &'static str) -> Self {
        Self { content }
    }

    /// Calculate the hash of the module content for use as a filename
    /// This uses a simple FNV-1a hash that can also be computed at compile time
    pub fn hash(&self) -> String {
        format!("{:x}", self.const_hash())
    }

    /// Const-compatible hash function (FNV-1a)
    pub const fn const_hash(&self) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET_BASIS;
        let mut i = 0;
        let bytes = self.content.as_bytes();
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        hash
    }
}

inventory::collect!(InlineJsModule);

impl JsFunctionSpec {
    pub const fn new(js_code: fn() -> String, type_info: fn() -> (Vec<String>, String)) -> Self {
        Self { js_code, type_info }
    }
}

inventory::collect!(JsFunctionSpec);

/// Registry of JS functions collected via inventory

pub struct FunctionRegistry {
    functions: String,
    function_specs: Vec<JsFunctionSpec>,
    /// Map of module path -> module content for inline_js modules
    modules: std::collections::HashMap<String, &'static str>,
}

pub static FUNCTION_REGISTRY: std::sync::LazyLock<FunctionRegistry> =
    std::sync::LazyLock::new(FunctionRegistry::collect_from_inventory);

impl FunctionRegistry {
    fn collect_from_inventory() -> Self {
        use std::fmt::Write;

        let mut modules = std::collections::HashMap::new();

        // Collect all inline JS modules and deduplicate by content hash
        for inline_js in inventory::iter::<InlineJsModule>() {
            let hash = inline_js.hash();
            let module_path = format!("snippets/{}.js", hash);
            // Only insert if we haven't seen this content before
            modules.entry(module_path).or_insert(inline_js.content);
        }

        // Collect all function specs
        let specs: Vec<_> = inventory::iter::<JsFunctionSpec>().copied().collect();

        // Build the script - load modules from wry:// handler before setting up function registry
        let mut script = String::new();

        // Wrap everything in an async IIFE to use await
        script.push_str("(async () => {\n");

        // Track which modules we've already imported (by hash)
        let mut imported_modules = std::collections::HashSet::new();

        // Load all inline_js modules from the wry handler (deduplicated by content hash)
        for inline_js in inventory::iter::<InlineJsModule>() {
            let hash = inline_js.hash();
            // Only import each unique module once
            if imported_modules.insert(hash.clone()) {
                // Dynamically import the module from wry://snippets/{hash}.js
                write!(
                    &mut script,
                    "  const module_{} = await import('wry://snippets/{}.js');\n",
                    hash, hash
                )
                .unwrap();
            }
        }

        // Now set up the function registry after all modules are loaded
        script.push_str("  window.setFunctionRegistry([");
        for (i, spec) in specs.iter().enumerate() {
            if i > 0 {
                script.push_str(",\n");
            }
            let (args, return_type) = (spec.type_info)();
            let js_code = (spec.js_code)();
            write!(
                &mut script,
                "window.createWrapperFunction([{}], {}, {})",
                args.join(", "),
                return_type,
                js_code
            )
            .unwrap();
        }
        script.push_str("]);\n");

        // Send a request to wry to notify that the function registry is ready
        script.push_str("  fetch('wry://ready', { method: 'POST', body: [] });\n");

        // Close the async IIFE
        script.push_str("})();\n");

        Self {
            functions: script,
            function_specs: specs,
            modules,
        }
    }

    /// Get a function by name from the registry
    pub fn get_function<F>(&self, spec: JsFunctionSpec) -> Option<JSFunction<F>>
    where
        F: 'static,
    {
        let index = self
            .function_specs
            .iter()
            .position(|s| s.js_code as usize == spec.js_code as usize)?;
        Some(JSFunction::new(index as _))
    }

    /// Get the initialization script
    pub fn script(&self) -> &str {
        &self.functions
    }

    /// Get the content of an inline_js module by path
    pub fn get_module(&self, path: &str) -> Option<&'static str> {
        self.modules.get(path).copied()
    }
}

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

/// Extension trait for Option to unwrap or throw a JS error.
/// This is API-compatible with wasm-bindgen's UnwrapThrowExt.
pub trait UnwrapThrowExt<T> {
    /// Unwrap the value or panic with a message.
    fn unwrap_throw(self) -> T;

    /// Unwrap the value or panic with a custom message.
    fn expect_throw(self, message: &str) -> T;
}

impl<T> UnwrapThrowExt<T> for Option<T> {
    fn unwrap_throw(self) -> T {
        self.expect("called `Option::unwrap_throw()` on a `None` value")
    }

    fn expect_throw(self, message: &str) -> T {
        self.expect(message)
    }
}

impl<T, E: std::fmt::Debug> UnwrapThrowExt<T> for Result<T, E> {
    fn unwrap_throw(self) -> T {
        self.expect("called `Result::unwrap_throw()` on an `Err` value")
    }

    fn expect_throw(self, message: &str) -> T {
        self.expect(message)
    }
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::Closure;
    pub use crate::UnwrapThrowExt;
    pub use crate::batch::batch;
    pub use crate::cast::JsCast;
    pub use crate::encode::{BatchableResult, BinaryDecode, BinaryEncode, TypeConstructor};
    pub use crate::function::JSFunction;
    pub use crate::lazy::JsThreadLocal;
    #[cfg(feature = "runtime")]
    pub use crate::runtime::{AppEvent, set_event_loop_proxy, shutdown, wait_for_js_result};
    pub use crate::value::JsValue;
    pub use crate::wasm_bindgen;
}
