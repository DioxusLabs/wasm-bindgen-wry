//! wry-bindgen - Runtime support for wasm-bindgen-style bindings over Wry's WebView
//!
//! This crate provides the runtime types and traits needed for the `#[wasm_bindgen]`
//! attribute macro to generate code that works with Wry's IPC protocol.
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`encode`] - Core encoding/decoding traits for Rust types
//! - [`function`] - JSFunction type for calling JavaScript functions
//! - [`mod@batch`] - Batching system for grouping multiple JS operations
//! - [`runtime`] - Event loop and runtime management

#![no_std]

pub extern crate alloc;
#[macro_use]
extern crate std;

pub mod batch;
mod cast;
pub mod convert;
pub mod encode;
pub mod function;
mod function_registry;
mod intern;
pub(crate) mod ipc;
mod js_helpers;
mod lazy;
pub mod object_store;
pub mod runtime;
mod value;
pub mod wry;

pub use intern::*;

/// Re-export of the Closure type for wasm-bindgen API compatibility.
/// Allows `use wasm_bindgen::closure::Closure;`
pub mod closure {
    pub use crate::Closure;
    pub use crate::WasmClosure;
}

/// Runtime module for wasm-bindgen compatibility.
/// This module provides the wbg_cast function used for type casting.
pub mod __rt {
    use crate::{
        JsFunctionSpec, LazyJsFunction,
        encode::{BatchableResult, BinaryEncode, EncodeTypeDef},
    };

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
        /// The identity cast function spec - registered once and reused by wbg_cast.
        /// This is the JS function `(a0) => a0` that passes values through unchanged.
        /// Type conversion is handled by Rust's encode/decode based on the type parameters.
        static IDENTITY_CAST_SPEC: JsFunctionSpec =
            JsFunctionSpec::new(|| alloc::string::String::from("(a0) => a0"));

        inventory::submit! {
            IDENTITY_CAST_SPEC
        }

        let func: LazyJsFunction<fn(From) -> To> = IDENTITY_CAST_SPEC.resolve_as();
        func.call(value)
    }
}

macro_rules! cast {
    (($from:ty => $to:ty) $val:expr) => {{ $crate::__rt::wbg_cast::<$from, $to>($val) }};
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

impl TryFrom<JsValue> for f64 {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        value.as_f64().ok_or(value)
    }
}

impl TryFrom<&JsValue> for f64 {
    type Error = JsValue;

    fn try_from(value: &JsValue) -> Result<Self, Self::Error> {
        value.as_f64().ok_or_else(|| value.clone())
    }
}

impl TryFrom<JsValue> for i128 {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        #[wasm_bindgen(crate = crate, inline_js = "export function BigIntAsI128(val) {
            if (typeof val !== 'bigint') {
                throw new Error('Value is not a BigInt');
            }
            return Number(val);
        }")]
        extern "C" {
            #[wasm_bindgen(js_name = "BigIntAsI128")]
            fn big_int_as_i128(val: &JsValue) -> Result<i128, JsValue>;
        }

        big_int_as_i128(&value)
    }
}

impl TryFrom<JsValue> for u128 {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        #[wasm_bindgen(crate = crate, inline_js = "export function BigIntAsU128(val) {
            if (typeof val !== 'bigint') {
                throw new Error('Value is not a BigInt');
            }
            if (val < 0n) {
                throw new Error('Value is negative');
            }
            return Number(val);
        }")]
        extern "C" {
            #[wasm_bindgen(js_name = "BigIntAsU128")]
            fn big_int_as_u128(val: &JsValue) -> Result<u128, JsValue>;
        }

        big_int_as_u128(&value)
    }
}

impl TryFrom<JsValue> for String {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        value.as_string().ok_or(value)
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
to_js_value!(u8);
from_js_value!(u8);
to_js_value!(u16);
from_js_value!(u16);
to_js_value!(u32);
from_js_value!(u32);
to_js_value!(u64);
to_js_value!(u128);
to_js_value!(f32);
from_js_value!(f32);
to_js_value!(f64);
to_js_value!(usize);
from_js_value!(usize);
to_js_value!(isize);
from_js_value!(isize);
impl From<&str> for JsValue {
    fn from(val: &str) -> Self {
        cast! {(String => JsValue) val.to_string()}
    }
}
impl From<&String> for JsValue {
    fn from(val: &String) -> Self {
        cast! {(String => JsValue) val.clone()}
    }
}
to_js_value!(String);
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
    pub fn new<M, F: IntoClosure<M, Self>>(f: F) -> Self {
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

    /// Create a `Closure` from a function that can only be called once,
    /// and return the underlying `JsValue` directly.
    ///
    /// This is a convenience method that combines `once` and `into_js_value`.
    pub fn once_into_js<F, M>(fn_once: F) -> JsValue
    where
        F: WasmClosureFnOnce<T, M>,
        T: Sized,
    {
        Closure::once(fn_once).into_js_value()
    }
}

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use core::ops::{Deref, DerefMut};
// Re-export core types
pub use cast::JsCast;
pub use lazy::JsThreadLocal;
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
#[repr(transparent)]
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
        static __FUNC: LazyJsFunction<fn(&str) -> JsValue> = __SPEC.resolve_as();
        JsError {
            value: __FUNC.call(message),
        }
    }
}

impl From<JsError> for JsValue {
    fn from(e: JsError) -> Self {
        e.value
    }
}

impl<T> From<Option<T>> for JsValue
where
    T: Into<JsValue>,
{
    fn from(s: Option<T>) -> JsValue {
        match s {
            Some(s) => s.into(),
            None => JsValue::undefined(),
        }
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
        // SAFETY: #[repr(transparent)] guarantees same layout
        unsafe { &*(val as *const JsValue as *const JsError) }
    }
}

// Re-export commonly used items
pub use batch::batch;
pub use encode::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};
pub use function::JSFunction;
pub use ipc::{DecodeError, DecodedData, EncodedData};
pub use runtime::{WryRuntime, start_app};

// Re-export the macros
pub use wry_bindgen_macro::link_to;
pub use wry_bindgen_macro::wasm_bindgen;

// Re-export inventory for macro use
pub use inventory;

use crate::encode::IntoClosure;

// Re-export function registry types
pub use function_registry::{
    InlineJsModule, JsClassMemberKind, JsClassMemberSpec, JsExportSpec, JsFunctionSpec,
    LazyJsFunction,
};

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
pub trait UnwrapThrowExt<T>: Sized {
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

impl<T, E> UnwrapThrowExt<T> for Result<T, E>
where
    E: core::fmt::Debug,
{
    fn unwrap_throw(self) -> T {
        self.expect("called `Result::unwrap_throw()` on an `Err` value")
    }

    fn expect_throw(self, message: &str) -> T {
        self.expect(message)
    }
}

#[cold]
#[inline(never)]
pub fn throw_val(s: JsValue) -> ! {
    panic!("{s:?}");
}

/// Throw a JS exception with the given message.
///
/// # Panics
/// This function always panics when running outside of WASM.
#[cold]
#[inline(never)]
pub fn throw_str(s: &str) -> ! {
    panic!("cannot throw JS exception when running outside of wasm: {s}");
}

/// Returns the number of live externref objects.
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn externref_heap_live_count() -> u32 {
    panic!("cannot introspect wasm memory when running outside of wasm")
}

/// Returns a handle to this Wasm instance's `WebAssembly.Module`.
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn module() -> JsValue {
    panic!("cannot introspect wasm memory when running outside of wasm")
}

/// Returns a handle to this Wasm instance's `WebAssembly.Instance.prototype.exports`.
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn exports() -> JsValue {
    panic!("cannot introspect wasm memory when running outside of wasm")
}

/// Returns a handle to this Wasm instance's `WebAssembly.Memory`.
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn memory() -> JsValue {
    panic!("cannot introspect wasm memory when running outside of wasm")
}

/// Returns a handle to this Wasm instance's `WebAssembly.Table` (indirect function table).
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn function_table() -> JsValue {
    panic!("cannot introspect wasm memory when running outside of wasm")
}

// Re-export extract_rust_handle from js_helpers
pub use js_helpers::extract_rust_handle;

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
    pub use crate::value::JsValue;
    pub use crate::wasm_bindgen;
}
