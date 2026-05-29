//! wry-bindgen - Runtime support for wasm-bindgen-style bindings over Wry's WebView
//!
//! This crate provides the runtime types and traits needed for the `#[wasm_bindgen]`
//! attribute macro to generate code that works with Wry's IPC protocol.
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`BinaryEncode`]/[`BinaryDecode`] - Core encoding/decoding traits for Rust types
//! - [`JSFunction`] - JSFunction type for calling JavaScript functions
//! - [`batch`] - Batching helpers for grouping multiple JS operations
//! - [`wry`] - Event loop and Wry integration

#![no_std]

#[doc(hidden)]
pub extern crate alloc;
#[macro_use]
extern crate std;

pub mod batch;
mod cast;
mod clamped;
pub mod closure;
pub mod convert;
mod encode;
mod erasure;
mod function;
mod function_registry;
mod id_allocator;
mod intern;
pub(crate) mod ipc;
#[macro_use]
mod wire;
#[doc(hidden)]
#[path = "rt.rs"]
pub mod __rt;
mod js_error;
mod js_helpers;
mod lazy;
mod object_store;
mod parent;
mod runtime;
pub mod sys;
mod try_from_js;
mod type_cache;
mod value;
pub mod wry;

pub use intern::*;

// Re-export core types
pub use crate::__rt::marker::ErasableGeneric;
pub use cast::JsCast;
pub use clamped::Clamped;
pub use closure::{
    Closure, IntoWasmClosure, IntoWasmClosureRef, IntoWasmClosureRefMut, MaybeUnwindSafe,
    ScopedClosure, WasmClosure, WasmClosureFnOnce, WasmClosureFnOnceAbort, WryWasmClosure,
};
pub use js_error::JsError;
pub use lazy::JsThreadLocal;
pub use value::JsValue;

pub use crate::__rt::{Ref, RefMut};
pub use parent::Parent;

pub use convert::{IntoJsGeneric, JsGeneric};
pub use encode::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};
pub use function::JSFunction;
pub use ipc::{DecodeError, DecodedData, EncodedData};
pub use sys::{JsOption, Null, Promising, Undefined};

// Re-export the macros
pub use wry_bindgen_macro::link_to;
pub use wry_bindgen_macro::wasm_bindgen;

/// Macro to register and call a JavaScript function.
///
/// This macro encapsulates the common pattern of:
/// 1. Creating a static JsFunctionSpec
/// 2. Submitting it to inventory
/// 3. Creating a LazyJsFunction with the given signature
/// 4. Calling the function with the provided arguments
///
/// # Usage
/// ```ignore
/// __wry_call_js_function!("(a, b) => a + b", fn(i32, i32) -> i32, (x, y))
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __wry_call_js_function {
    ($js_code:expr, $fn_type:ty, ($($args:expr),*)) => {{
        static __FUNC: $crate::__rt::LazyJsFunction<$fn_type> =
            $crate::__wry_submit_js_function!($js_code);

        __FUNC.call($($args),*)
    }};
}

/// Macro to register and call a JavaScript function.
///
/// This macro encapsulates the common pattern of:
/// 1. Creating a static JsFunctionSpec
/// 2. Submitting it to inventory
/// 3. Creating a LazyJsFunction with the given signature
///
/// # Usage
/// ```ignore
/// __wry_submit_js_function!("(a, b) => a + b")
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __wry_submit_js_function {
    ($js_code:expr) => {{
        static __SPEC: $crate::__rt::JsFunctionSpec =
            $crate::__rt::JsFunctionSpec::new(|| $crate::alloc::format!($js_code));

        $crate::__rt::inventory::submit! {
            __SPEC
        }

        __SPEC.resolve_as()
    }};
}

/// Extension trait for Option to unwrap or throw a JS error.
/// This is API-compatible with wasm-bindgen's UnwrapThrowExt.
pub trait UnwrapThrowExt<T>: Sized {
    /// Unwrap the value or panic with a message.
    ///
    /// Has a default impl (delegating to [`expect_throw`](Self::expect_throw)) to
    /// match upstream wasm-bindgen, so downstream impls only need `expect_throw`.
    #[cfg_attr(any(debug_assertions, not(target_family = "wasm")), track_caller)]
    fn unwrap_throw(self) -> T {
        if cfg!(all(debug_assertions, target_family = "wasm")) {
            let loc = core::panic::Location::caller();
            let msg = alloc::format!(
                "called `{}::unwrap_throw()` ({}:{}:{})",
                core::any::type_name::<Self>(),
                loc.file(),
                loc.line(),
                loc.column()
            );
            self.expect_throw(&msg)
        } else {
            self.expect_throw("called `unwrap_throw()`")
        }
    }

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

/// Renamed to [`throw_str`].
#[cold]
#[inline(never)]
#[deprecated(note = "renamed to `throw_str`")]
#[doc(hidden)]
pub fn throw(s: &str) -> ! {
    throw_str(s)
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

/// Returns a handle to this Wasm instance's `WebAssembly.Instance`.
///
/// # Panics
/// This function always panics when running outside of WASM.
pub fn instance() -> JsValue {
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

/// Legacy wrapper for imported statics.
///
/// This type implements `Deref` to the inner type so it's typically used as if
/// it were `&T`. Prefer `#[wasm_bindgen(thread_local_v2)]` and [`JsThreadLocal`].
#[deprecated = "use with `#[wasm_bindgen(thread_local_v2)]` instead"]
pub struct JsStatic<T: 'static> {
    #[doc(hidden)]
    pub __inner: &'static std::thread::LocalKey<T>,
}

#[allow(deprecated)]
impl<T: 'static> core::ops::Deref for JsStatic<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.__inner.with(|ptr| &*(ptr as *const T)) }
    }
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::Clamped;
    pub use crate::JsCast;
    pub use crate::JsError;
    pub use crate::JsValue;
    pub use crate::UnwrapThrowExt;
    pub use crate::WasmClosure;
    pub use crate::closure::{Closure, ScopedClosure};
    pub use crate::convert::Upcast;
    pub use crate::convert::{IntoJsGeneric, JsGeneric, UpcastFrom};
    pub use crate::wasm_bindgen;
    pub use crate::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};
    pub use crate::{JSFunction, JsThreadLocal};
    pub use crate::{JsOption, Null, Promising, Undefined};
}
