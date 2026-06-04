//! wry-bindgen - Runtime support for wasm-bindgen-style bindings over Wry's WebView
//!
//! This crate provides the runtime types and traits needed for the `#[wasm_bindgen]`
//! attribute macro to generate code that works with Wry's IPC protocol.
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`JsValue`] - Opaque references to JavaScript values
//! - [`closure`] - Rust closures passed to JavaScript
//! - [`convert`] - wasm-bindgen-compatible conversion marker traits
//! - [`sys`] - JavaScript semantic helper types

#![no_std]

#[doc(hidden)]
pub extern crate alloc;
#[macro_use]
extern crate std;

mod cast;
pub mod closure;
pub mod convert;
pub mod describe;
mod encode;
mod erasure;
#[doc(hidden)]
pub mod handler;
pub(crate) mod ipc;
#[macro_use]
mod wire;
#[doc(hidden)]
#[path = "rt.rs"]
pub mod __rt;
mod js_error;
mod js_helpers;
mod object_store;
mod parent;
pub mod sys;
mod try_from_js;
mod value;

// Re-export core types
pub use crate::__rt::marker::ErasableGeneric;
pub use cast::JsCast;
pub use closure::{
    Closure, IntoWasmClosure, IntoWasmClosureRef, IntoWasmClosureRefMut, MaybeUnwindSafe,
    ScopedClosure, WasmClosure, WasmClosureFnOnce, WasmClosureFnOnceAbort, WryWasmClosure,
};
pub use js_error::JsError;
pub use value::JsValue;
pub use wry_bindgen_core::JsThreadLocal;

pub use parent::Parent;
#[doc(inline)]
pub use wry_bindgen_core::Clamped;

pub use convert::{IntoJsGeneric, JsGeneric};

// Re-export the macros
pub use wry_bindgen_macro::link_to;
pub use wry_bindgen_macro::wasm_bindgen;

#[inline]
pub fn intern(s: &str) -> &str {
    s
}

#[inline]
pub fn unintern(_: &str) {}

/// Macro to register and call a JavaScript function.
///
/// This macro encapsulates the common pattern of:
/// 1. Creating a static JsFunctionSpec
/// 2. Submitting it to inventory
/// 3. Creating a JsFunction with the given signature
/// 4. Calling the function with the provided arguments
///
/// # Usage
/// ```ignore
/// __wry_call_js_function!("(a, b) => a + b", fn(i32, i32) -> i32, (x, y))
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __wry_call_js_function {
    (module = $module:expr, $js_code:expr, $fn_type:ty, ($($args:expr),*)) => {{
        static __FUNC: $crate::__rt::JsFunction<$fn_type> =
            $crate::__wry_submit_js_function!(module = $module, $js_code);

        __FUNC.call($($args),*)
    }};
    ($js_code:expr, $fn_type:ty, ($($args:expr),*)) => {{
        static __FUNC: $crate::__rt::JsFunction<$fn_type> =
            $crate::__wry_submit_js_function!($js_code);

        __FUNC.call($($args),*)
    }};
}

/// Macro to register and call a JavaScript function.
///
/// This macro encapsulates the common pattern of:
/// 1. Creating a static JsFunctionSpec
/// 2. Submitting it to inventory
/// 3. Creating a JsFunction with the given signature
///
/// # Usage
/// ```ignore
/// __wry_submit_js_function!("(a, b) => a + b")
/// ```
#[macro_export]
#[doc(hidden)]
macro_rules! __wry_submit_js_function {
    (module = $module:expr, $js_code:expr) => {{
        static __SPEC: $crate::__rt::JsFunctionSpec =
            $crate::__rt::JsFunctionSpec::with_module($module, |__wry_module| {
                $crate::alloc::format!($js_code, __wry_module = __wry_module)
            });

        $crate::__rt::inventory::submit! {
            __SPEC
        }

        $crate::__rt::JsFunction::new(__SPEC)
    }};
    ($js_code:expr) => {{
        static __SPEC: $crate::__rt::JsFunctionSpec =
            $crate::__rt::JsFunctionSpec::new(|| $crate::alloc::format!($js_code));

        $crate::__rt::inventory::submit! {
            __SPEC
        }

        $crate::__rt::JsFunction::new(__SPEC)
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
    pub use crate::JsCast;
    pub use crate::JsError;
    pub use crate::JsValue;
    pub use crate::UnwrapThrowExt;
    pub use crate::closure::{Closure, ScopedClosure};
    pub use crate::convert::Upcast;
    pub use crate::wasm_bindgen;
    #[doc(hidden)]
    pub use wry_bindgen_macro::__wasm_bindgen_class_marker;
}
