//! Runtime compatibility hooks used by generated wasm-bindgen-style code.

use crate::{__wry_submit_js_function, JsValue};
use core::convert::Infallible;

// Re-export the std-family crates so generated code can spell them as
// `wasm_bindgen::__rt::{core,alloc,std}::...`. The macro crate is always in scope
// where `#[wasm_bindgen]` is used, so these paths resolve in any edition —
// including edition-2015 crates that lack `extern crate core` in their root.
#[doc(hidden)]
pub extern crate alloc;
#[doc(hidden)]
pub extern crate core;
#[doc(hidden)]
pub extern crate std;

#[doc(hidden)]
pub use crate::encode::{
    BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef, JsRef, JsRefEncode, TypeDef,
};
#[doc(hidden)]
pub use crate::ipc::{DecodeError, DecodedData, EncodedData};
#[doc(hidden)]
pub use crate::js_helpers::js_extract_rust_handle as extract_rust_handle;
#[doc(hidden)]
pub use inventory;
#[doc(hidden)]
pub use wry_bindgen_core::Runtime;
#[doc(hidden)]
pub use wry_bindgen_core::{
    JsClassMemberKind, JsClassMemberSpec, JsClassSpec, JsExportSpecRegistration, JsFunction,
    JsFunctionSpec, JsModuleSpec, JsReexportSpec, LazyCell,
};
#[doc(hidden)]
pub use wry_bindgen_core::{link_to_raw_specifier, register_linked_module};

#[doc(hidden)]
pub mod object_store {
    pub use crate::object_store::{
        ObjectHandle, ObjectRefAnchor, ObjectRefMutAnchor, checkout_object_mut,
        checkout_object_ref, create_js_wrapper, drop_object, insert_object, object_is,
        remove_object,
    };
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WasmWord(pub u32);

/// Internal helper for upstream-compatible `#[wasm_bindgen(main)]` expansion.
pub struct MainWrapper<T>(pub Option<T>);

/// Internal trait for upstream-compatible `#[wasm_bindgen(main)]` expansion.
pub trait Main {
    fn __wasm_bindgen_main(&mut self);
}

impl Main for &mut &mut MainWrapper<()> {
    #[inline]
    fn __wasm_bindgen_main(&mut self) {}
}

impl Main for &mut &mut MainWrapper<Infallible> {
    #[inline]
    fn __wasm_bindgen_main(&mut self) {}
}

impl<E: Into<JsValue>> Main for &mut &mut MainWrapper<Result<(), E>> {
    #[inline]
    fn __wasm_bindgen_main(&mut self) {
        if let Err(error) = self.0.take().unwrap() {
            crate::throw_val(error.into());
        }
    }
}

impl<E: core::fmt::Debug> Main for &mut MainWrapper<Result<(), E>> {
    #[inline]
    fn __wasm_bindgen_main(&mut self) {
        if let Err(error) = self.0.take().unwrap() {
            crate::throw_str(&alloc::format!("{error:?}"));
        }
    }
}

pub struct Ref<'b, T: ?Sized + 'b> {
    pub(crate) inner: core::cell::Ref<'b, T>,
}

impl<T: ?Sized> core::ops::Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ?Sized> core::borrow::Borrow<T> for Ref<'_, T> {
    fn borrow(&self) -> &T {
        self
    }
}

pub struct RefMut<'b, T: ?Sized + 'b> {
    pub(crate) inner: core::cell::RefMut<'b, T>,
}

impl<T: ?Sized> core::ops::Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ?Sized> core::ops::DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: ?Sized> core::borrow::Borrow<T> for RefMut<'_, T> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T: ?Sized> core::borrow::BorrowMut<T> for RefMut<'_, T> {
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

pub mod marker {
    /// Marker for types whose generic parameters erase to one stable runtime representation.
    ///
    /// # Safety
    /// Implementors must have the same runtime representation as `Repr`.
    pub unsafe trait ErasableGeneric {
        type Repr: 'static;
    }

    unsafe impl<T: ErasableGeneric> ErasableGeneric for &T {
        type Repr = &'static T::Repr;
    }

    unsafe impl<T: ErasableGeneric> ErasableGeneric for &mut T {
        type Repr = &'static mut T::Repr;
    }

    /// Marker for owned generic values whose representation matches a concrete target.
    pub trait ErasableGenericOwn<ConcreteTarget>: ErasableGeneric {}

    impl<T, ConcreteTarget> ErasableGenericOwn<ConcreteTarget> for T
    where
        ConcreteTarget: ErasableGeneric,
        T: ErasableGeneric<Repr = <ConcreteTarget as ErasableGeneric>::Repr>,
    {
    }

    /// Marker for borrowed generic values whose representation matches a concrete target.
    pub trait ErasableGenericBorrow<Target: ?Sized> {}

    impl<'a, T: ?Sized + 'a, ConcreteTarget: ?Sized + 'static> ErasableGenericBorrow<ConcreteTarget>
        for T
    where
        &'static ConcreteTarget: ErasableGeneric,
        &'a T: ErasableGeneric<Repr = <&'static ConcreteTarget as ErasableGeneric>::Repr>,
    {
    }

    /// Marker for mutably borrowed generic values whose representation matches a concrete target.
    pub trait ErasableGenericBorrowMut<Target: ?Sized> {}

    impl<'a, T: ?Sized + 'a, ConcreteTarget: ?Sized + 'static>
        ErasableGenericBorrowMut<ConcreteTarget> for T
    where
        &'static mut ConcreteTarget: ErasableGeneric,
        &'a mut T: ErasableGeneric<Repr = <&'static mut ConcreteTarget as ErasableGeneric>::Repr>,
    {
    }
}

/// Cast between types via the binary protocol.
///
/// This is the wry-bindgen equivalent of wasm-bindgen's wbg_cast.
/// It encodes `value` using From's BinaryEncode, sends to JS as identity,
/// and decodes the result using To's BinaryDecode.
#[inline]
pub fn wbg_cast<From, To>(value: From) -> To
where
    From: BinaryEncode + EncodeTypeDef,
    To: BatchableResult + EncodeTypeDef + 'static,
{
    let func: JsFunction<fn(From) -> To> = __wry_submit_js_function!("(a0) => a0");
    func.call(value)
}

#[doc(hidden)]
pub fn set_on_abort(_f: fn()) -> Option<fn()> {
    None
}

#[doc(hidden)]
pub fn schedule_reinit() {}

/// Convert a panic value into a JsValue error.
///
/// This is used by wasm-bindgen-futures to convert Rust panics into JS errors.
#[cfg(feature = "std")]
pub fn panic_to_panic_error(val: std::boxed::Box<dyn std::any::Any + Send>) -> JsValue {
    let maybe_panic_msg: Option<&str> = if let Some(s) = val.downcast_ref::<&str>() {
        Some(s)
    } else if let Some(s) = val.downcast_ref::<std::string::String>() {
        Some(s)
    } else {
        None
    };
    // Create an Error object with the panic message
    JsValue::from_str(maybe_panic_msg.unwrap_or("Rust panic"))
}
