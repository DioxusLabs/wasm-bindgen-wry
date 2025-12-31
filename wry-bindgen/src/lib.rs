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

#![no_std]

pub extern crate alloc;
#[macro_use]
extern crate std;

pub mod batch;
mod cast;
pub mod convert;
pub mod encode;
pub mod function;
pub mod ipc;
mod js_helpers;
mod lazy;
pub mod object_store;
pub mod runtime;
mod value;

/// Re-export of the Closure type for wasm-bindgen API compatibility.
/// Allows `use wasm_bindgen::closure::Closure;`
pub mod closure {
    pub use crate::Closure;
    pub use crate::WasmClosure;
}

/// The identity cast function spec - registered once and reused by wbg_cast.
/// This is the JS function `(a0) => a0` that passes values through unchanged.
/// Type conversion is handled by Rust's encode/decode based on the type parameters.
pub static IDENTITY_CAST_SPEC: JsFunctionSpec =
    JsFunctionSpec::new(|| alloc::string::String::from("(a0) => a0"));

inventory::submit! {
    IDENTITY_CAST_SPEC
}

/// Runtime module for wasm-bindgen compatibility.
/// This module provides the wbg_cast function used for type casting.
pub mod __rt {
    use crate::encode::{BatchableResult, BinaryEncode, EncodeTypeDef};

    /// Cast between types via the binary protocol.
    ///
    /// This is the wry-bindgen equivalent of wasm-bindgen's wbg_cast.
    /// It encodes `value` using From's BinaryEncode, sends to JS as identity,
    /// and decodes the result using To's BinaryDecode.
    #[inline]
    pub fn wbg_cast<From, To>(value: From) -> To
    where
        From: BinaryEncode + EncodeTypeDef,
        To: BatchableResult + EncodeTypeDef,
    {
        let func: crate::JSFunction<fn(From) -> To> = crate::FUNCTION_REGISTRY
            .get_function(crate::IDENTITY_CAST_SPEC)
            .expect("Identity cast function not found");
        func.call(value)
    }
}

macro_rules! cast {
    (($from:ty => $to:ty) $val:expr) => {{
        $crate::__rt::wbg_cast::<$from, $to>($val)
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

impl TryFrom<JsValue> for u64 {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        eprintln!("TryFrom<JsValue> for u64 is likely wrong");
        #[wasm_bindgen(crate = crate, inline_js = "export function BigIntAsU64(val) {
            if (typeof val !== 'bigint') {
                throw new Error('Value is not a BigInt');
            }
            return Number(val);
        }")]
        extern "C" {
            #[wasm_bindgen(js_name = "BigIntAsU64")]
            fn big_int_as_u64(val: &JsValue) -> Result<u64, JsValue>;
        }

        big_int_as_u64(&value)
    }
}

impl TryFrom<JsValue> for i64 {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        eprintln!("TryFrom<JsValue> for u64 is likely wrong");
        #[wasm_bindgen(crate = crate, inline_js = "export function BigIntAsU64(val) {
            if (typeof val !== 'bigint') {
                throw new Error('Value is not a BigInt');
            }
            return Number(val);
        }")]
        extern "C" {
            #[wasm_bindgen(js_name = "BigIntAsU64")]
            fn big_int_as_i64(val: &JsValue) -> Result<i64, JsValue>;
        }

        big_int_as_i64(&value)
    }
}

to_js_value!(i8);
from_js_value!(i8);
to_js_value!(i16);
from_js_value!(i16);
to_js_value!(i32);
from_js_value!(i32);
to_js_value!(i64);
to_js_value!(i128);
from_js_value!(i128);
to_js_value!(u8);
from_js_value!(u8);
to_js_value!(u16);
from_js_value!(u16);
to_js_value!(u32);
from_js_value!(u32);
to_js_value!(u64);
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
// Manual impl for &str since it has a lifetime and wbg_cast requires 'static
impl From<&str> for JsValue {
    fn from(val: &str) -> Self {
        cast! {(String => JsValue) val.to_string()}
    }
}
to_js_value!(String);
from_js_value!(String);
to_js_value!(());
from_js_value!(());

/// Closure type for passing Rust closures to JavaScript.
pub struct Closure<T: ?Sized> {
    // careful: must be Box<T> not just T because unsized PhantomData
    // seems to have weird interaction with Pin<>
    _phantom: core::marker::PhantomData<Box<T>>,
    pub(crate) value: JsValue,
}

impl<T: ?Sized> Closure<T> {
    pub fn new<M, F: IntoClosure<M, Output = Self>>(f: F) -> Self {
        f.into_closure()
    }

    /// Create a `Closure` from a function that can only be called once.
    ///
    /// Since we have no way of enforcing that JS cannot attempt to call this
    /// `FnOnce` more than once, this produces a `Closure<dyn FnMut(A...) -> R>`
    /// that will panic if called more than once.
    pub fn once<F, M>(fn_once: F) -> Closure<T>
    where
        F: WasmClosureFnOnce<T, M>,
    {
        fn_once.into_closure()
    }

    /// Forgets the closure, leaking it.
    pub fn forget(self) {
        core::mem::forget(self);
    }
}

/// A trait for converting an `FnOnce(A...) -> R` into a `Closure<dyn FnMut(A...) -> R>`.
#[doc(hidden)]
pub trait WasmClosureFnOnce<T: ?Sized, M>: Sized + 'static {
    fn into_closure(self) -> Closure<T>;
}

impl<T: ?Sized> AsRef<JsValue> for Closure<T> {
    fn as_ref(&self) -> &JsValue {
        &self.value
    }
}

impl<T: ?Sized> core::fmt::Debug for Closure<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Closure")
            .field("value", &self.value)
            .finish()
    }
}

/// Trait for closure types that can be wrapped and passed to JavaScript.
/// This trait is implemented for all `dyn FnMut(...)` variants.
pub trait WasmClosure<M> {
    /// Create a Closure from a boxed closure.
    fn into_js_closure(boxed: Box<Self>) -> Closure<Self>;
}

impl<T: ?Sized> Closure<T> {
    /// Wrap a boxed closure to create a `Closure`.
    ///
    /// This is the classic wasm-bindgen API for creating closures from boxed trait objects.
    pub fn wrap<M>(data: Box<T>) -> Closure<T>
    where
        T: WasmClosure<M>,
    {
        T::into_js_closure(data)
    }

    /// Converts the `Closure` into a `JsValue`.
    pub fn into_js_value(self) -> JsValue {
        let value = core::mem::ManuallyDrop::new(self);
        // Clone the value to get ownership without triggering drop
        value.value.clone()
    }
}

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
// Re-export core types
pub use cast::JsCast;
pub use convert::{FromWasmAbi, IntoWasmAbi};
pub use lazy::JsThreadLocal;
use once_cell::sync::Lazy;
pub use value::JsValue;

/// A wrapper type around slices and vectors for binding the `Uint8ClampedArray` in JS.
///
/// Supported inner types:
/// * `Clamped<&[u8]>`
/// * `Clamped<&mut [u8]>`
/// * `Clamped<Vec<u8>>`
#[derive(Copy, Clone, PartialEq, Debug, Eq)]
pub struct Clamped<T>(pub T);

impl<T> Deref for Clamped<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Clamped<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// A JavaScript Error object.
///
/// This type is used to create JavaScript Error objects that can be thrown or returned.
#[derive(Debug)]
pub struct JsError {
    value: JsValue,
}

impl JsError {
    /// Create a new JavaScript Error with the given message.
    pub fn new(message: &str) -> Self {
        // Create JS Error via helper function
        static __SPEC: JsFunctionSpec =
            JsFunctionSpec::new(|| "(msg) => new Error(msg)".to_string());
        inventory::submit! {
            __SPEC
        }
        let func: JSFunction<fn(&str) -> JsValue> = FUNCTION_REGISTRY
            .get_function(__SPEC)
            .expect("Function not found: Error constructor");
        JsError {
            value: func.call(message),
        }
    }
}

impl From<JsError> for JsValue {
    fn from(e: JsError) -> Self {
        e.value
    }
}

impl AsRef<JsValue> for JsError {
    fn as_ref(&self) -> &JsValue {
        &self.value
    }
}

impl core::fmt::Display for JsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "JsError")
    }
}

impl core::error::Error for JsError {}

impl JsCast for JsError {
    fn instanceof(val: &JsValue) -> bool {
        crate::js_helpers::js_is_error(val)
    }

    fn unchecked_from_js(val: JsValue) -> Self {
        JsError { value: val }
    }

    fn unchecked_from_js_ref(val: &JsValue) -> &Self {
        // This is unsafe but matches wasm-bindgen behavior
        unsafe { &*(val as *const JsValue as *const JsError) }
    }
}

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

use crate::encode::IntoClosure;

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

/// Specification for an exported Rust function/method callable from JavaScript.
///
/// This is used by the `#[wasm_bindgen]` macro when exporting structs and impl blocks.
/// Each export is registered via inventory and collected at runtime.
pub struct JsExportSpec {
    /// The export name (e.g., "MyStruct::new", "MyStruct::method")
    pub name: &'static str,
    /// Handler function that decodes arguments, calls the Rust function, and encodes the result
    pub handler: fn(&mut DecodedData) -> Result<EncodedData, alloc::string::String>,
}

impl JsExportSpec {
    pub const fn new(
        name: &'static str,
        handler: fn(&mut DecodedData) -> Result<EncodedData, alloc::string::String>,
    ) -> Self {
        Self { name, handler }
    }
}

inventory::collect!(JsExportSpec);

/// Registry of JS functions collected via inventory

pub struct FunctionRegistry {
    functions: String,
    function_specs: Vec<JsFunctionSpec>,
    /// Map of module path -> module content for inline_js modules
    modules: alloc::collections::BTreeMap<String, &'static str>,
}

pub static FUNCTION_REGISTRY: Lazy<FunctionRegistry> =
    Lazy::new(FunctionRegistry::collect_from_inventory);

impl FunctionRegistry {
    fn collect_from_inventory() -> Self {
        use core::fmt::Write;

        let mut modules = alloc::collections::BTreeMap::new();

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
        let mut imported_modules = alloc::collections::BTreeSet::new();

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

impl<T, E: core::fmt::Debug> UnwrapThrowExt<T> for Result<T, E> {
    fn unwrap_throw(self) -> T {
        self.expect("called `Result::unwrap_throw()` on an `Err` value")
    }

    fn expect_throw(self, message: &str) -> T {
        self.expect(message)
    }
}

pub fn throw_str(message: &str) -> ! {
    panic!("{}", message);
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::Clamped;
    pub use crate::Closure;
    pub use crate::JsError;
    pub use crate::UnwrapThrowExt;
    pub use crate::WasmClosure;
    pub use crate::batch::batch;
    pub use crate::cast::JsCast;
    pub use crate::encode::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};
    pub use crate::function::JSFunction;
    pub use crate::lazy::JsThreadLocal;
    pub use crate::runtime::{AppEvent, set_event_loop_proxy, shutdown, wait_for_js_result};
    pub use crate::value::JsValue;
    pub use crate::wasm_bindgen;
}
