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
pub mod runtime;
mod value;

/// Runtime module for wasm-bindgen compatibility.
/// This module provides stubs for wasm-specific functions that js-sys uses directly.
pub mod __rt {
    /// Stub for wasm memory casting - not supported in wry-bindgen.
    /// This function is only called for TypedArray::view() which requires wasm memory.
    #[inline]
    pub unsafe fn wbg_cast<From: ?Sized, To>(_val: &From) -> To {
        todo!()
    }
}

macro_rules! cast {
    (($from:ty => $to:ty) $val:expr) => {{
        static __SPEC: $crate::JsFunctionSpec =
            $crate::JsFunctionSpec::new(|| "(a0) => a0".to_string());
        inventory::submit! {
            __SPEC
        }
        let func: $crate::JSFunction<fn($from) -> $to> = $crate::FUNCTION_REGISTRY
            .get_function(__SPEC)
            .expect("Function not found: new_function");
        func.call($val)
    }};
}

macro_rules! to_js_value {
    ($ty:ty) => {
        impl From<$ty> for $crate::JsValue {
            fn from(val: $ty) -> Self {
                cast! {($ty => $crate::JsValue) val}
            }
        }
    };
}

macro_rules! from_js_value {
    ($ty:ty) => {
        impl From<$crate::JsValue> for $ty {
            fn from(val: $crate::JsValue) -> Self {
                cast! {($crate::JsValue => $ty) val}
            }
        }
    };
}

to_js_value!(i8);
from_js_value!(i8);
to_js_value!(i16);
from_js_value!(i16);
to_js_value!(i32);
from_js_value!(i32);
to_js_value!(i64);
from_js_value!(i64);
to_js_value!(i128);
from_js_value!(i128);
to_js_value!(u8);
from_js_value!(u8);
to_js_value!(u16);
from_js_value!(u16);
to_js_value!(u32);
from_js_value!(u32);
to_js_value!(u64);
from_js_value!(u64);
to_js_value!(u128);
from_js_value!(u128);
to_js_value!(f32);
from_js_value!(f32);
to_js_value!(f64);
from_js_value!(f64);
to_js_value!(usize);
from_js_value!(usize);
to_js_value!(isize);
from_js_value!(isize);
to_js_value!(&str);
to_js_value!(String);
from_js_value!(String);
to_js_value!(());
from_js_value!(());

/// Closure type for passing Rust closures to JavaScript.
pub struct Closure<T: ?Sized> {
    _phantom: std::marker::PhantomData<T>,
    pub(crate) value: JsValue,
}

impl<T: ?Sized> Closure<T> {
    pub fn new<F: Into<Closure<T>>>(f: F) -> Self {
        f.into()
    }

    /// Create a `Closure` from a function that can only be called once.
    ///
    /// Since we have no way of enforcing that JS cannot attempt to call this
    /// `FnOnce` more than once, this produces a `Closure<dyn FnMut(A...) -> R>`
    /// that will panic if called more than once.
    pub fn once<F, A, R>(fn_once: F) -> Closure<T>
    where
        F: WasmClosureFnOnce<T, A, R>,
    {
        fn_once.into_closure()
    }

    /// Forgets the closure, leaking it.
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

/// A trait for converting an `FnOnce(A...) -> R` into a `Closure<dyn FnMut(A...) -> R>`.
#[doc(hidden)]
pub trait WasmClosureFnOnce<T: ?Sized, A, R>: Sized + 'static {
    fn into_closure(self) -> Closure<T>;
}

impl<T: ?Sized> AsRef<JsValue> for Closure<T> {
    fn as_ref(&self) -> &JsValue {
        &self.value
    }
}

// Re-export core types
pub use cast::JsCast;
pub use lazy::JsThreadLocal;
pub use value::JsValue;

// Re-export commonly used items
pub use batch::batch;
pub use encode::{
    BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef, TYPE_CACHED, TYPE_FULL,
};
pub use function::JSFunction;
pub use ipc::{
    DecodeError, DecodedData, DecodedVariant, EncodedData, IPCMessage, MessageType, decode_data,
};
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
    pub const fn new(js_code: fn() -> String) -> Self {
        Self { js_code }
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
        // Store raw JS functions - type info will be passed at call time
        script.push_str("  window.setFunctionRegistry([");
        for (i, spec) in specs.iter().enumerate() {
            if i > 0 {
                script.push_str(",\n");
            }
            let js_code = (spec.js_code)();
            write!(&mut script, "{}", js_code).unwrap();
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
    pub use crate::encode::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};
    pub use crate::function::JSFunction;
    pub use crate::lazy::JsThreadLocal;
    pub use crate::runtime::{AppEvent, set_event_loop_proxy, shutdown, wait_for_js_result};
    pub use crate::value::JsValue;
    pub use crate::wasm_bindgen;
}
