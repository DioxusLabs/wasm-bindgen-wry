//! Closure compatibility types and traits.

use alloc::boxed::Box;

use crate::JsValue;
use crate::encode::{BinaryEncode, EncodeTypeDef};
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use crate::object_store::insert_object;
use wry_bindgen_core::{CallbackKey, ObjectHandle, RustCallback};

/// Borrowed or owned closure handle for passing Rust closures to JavaScript.
pub struct ScopedClosure<'a, T: ?Sized> {
    // careful: must be Box<T> not just T because unsized PhantomData
    // seems to have weird interaction with Pin<>
    pub(crate) _phantom: core::marker::PhantomData<(&'a (), crate::alloc::boxed::Box<T>)>,
    pub(crate) callback: CallbackOwnership,
    pub(crate) value: crate::JsValue,
}

/// Who is responsible for disposing the JS-side `RustFunction` wrapper that
/// backs a `ScopedClosure`. Encoding an owned `Closure` by value detaches it
/// because JavaScript takes ownership; borrowed encodes keep Rust ownership.
/// Destructors only dispose in the `Owned` state.
#[derive(Clone, Copy)]
pub(crate) enum CallbackOwnership {
    /// No Rust callback backing this closure (e.g., wrapping a raw JS function).
    None,
    /// Rust owns the callback; `ScopedClosure::drop` disposes it.
    Owned,
    /// Ownership has been handed off. No dispose on drop, but encoders still
    /// flush so JS receives the callable before the call that needs it.
    Detached,
}

impl CallbackOwnership {
    pub(crate) fn owned(_handle: ObjectHandle) -> Self {
        Self::Owned
    }

    /// Encoding this closure to JS requires an immediate flush so the JS
    /// side has the callable ready.
    pub(crate) fn needs_flush(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Transition `Owned` → `Detached`. No-op for `None` / `Detached`.
    pub(crate) fn detach(&mut self) {
        if matches!(self, Self::Owned) {
            *self = Self::Detached;
        }
    }
}

/// Owned closure handle. This follows upstream wasm-bindgen's alias direction.
pub type Closure<T> = ScopedClosure<'static, T>;

use crate::__rt::WasmWord;

/// Drop hook exported for compatibility with wasm-bindgen-generated code.
///
/// # Safety
///
/// This is an ABI entry point called by generated glue code. The arguments
/// must be the encoded closure metadata expected by that glue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wbindgen_destroy_closure(a: WasmWord, b: WasmWord) {
    let _ = (a, b);
}

impl<T: ?Sized> ScopedClosure<'static, T> {
    /// Wrap a raw closure. Only for use by generated code.
    pub(crate) fn wrap_encode_decode<FnPtr>(
        encode_decode: impl Fn(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError> + 'static,
    ) -> Self
    where
        CallbackKey<FnPtr>: BinaryEncode + EncodeTypeDef,
    {
        let key = insert_object(RustCallback::new_fn(encode_decode));
        let value =
            crate::__rt::wbg_cast::<CallbackKey<FnPtr>, crate::JsValue>(CallbackKey::new(key));
        Self {
            _phantom: core::marker::PhantomData,
            callback: crate::closure::CallbackOwnership::owned(key),
            value,
        }
    }

    /// Wrap a raw closure. Only for use by generated code.
    pub(crate) fn wrap_encode_decode_mut<FnPtr>(
        encode_decode: impl FnMut(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError>
        + 'static,
    ) -> Self
    where
        CallbackKey<FnPtr>: BinaryEncode + EncodeTypeDef,
    {
        let key = insert_object(RustCallback::new_fn_mut(encode_decode));
        let value =
            crate::__rt::wbg_cast::<CallbackKey<FnPtr>, crate::JsValue>(CallbackKey::new(key));
        Self {
            _phantom: core::marker::PhantomData,
            callback: crate::closure::CallbackOwnership::owned(key),
            value,
        }
    }

    /// Wrap a raw one-shot closure. Only for use by generated code.
    pub(crate) fn wrap_once_encode_decode_mut<FnPtr>(
        mut encode_decode: impl FnMut(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError>
        + 'static,
    ) -> Self
    where
        CallbackKey<FnPtr>: BinaryEncode + EncodeTypeDef,
    {
        let handle_cell = alloc::rc::Rc::new(core::cell::Cell::new(None::<ObjectHandle>));
        let handle_for_callback = handle_cell.clone();
        let key = insert_object(RustCallback::new_fn_mut(move |decoder, encoder| {
            let result = encode_decode(decoder, encoder);
            // Dispose the one-shot callback whether or not decoding succeeded.
            if let Some(handle) = handle_for_callback.take() {
                handle.drop_rust_object();
            }
            result
        }));
        handle_cell.set(Some(key));
        let value = crate::__rt::wbg_cast::<CallbackKey<FnPtr>, crate::JsValue>(
            CallbackKey::js_owned_once(key),
        );
        // Once-cells dispose themselves after the first call, but they still
        // need Rust-side ownership while a `Closure` handle exists. Promise
        // adapters store resolve/reject once-closures and rely on dropping the
        // pair after the first completion to dispose the unused callback.
        Self {
            _phantom: core::marker::PhantomData,
            callback: crate::closure::CallbackOwnership::owned(key),
            value,
        }
    }
}

impl<'a, T> ScopedClosure<'a, T>
where
    T: ?Sized + WasmClosure,
{
    /// Returns the JavaScript function value for this closure.
    pub fn as_js_value(&self) -> &JsValue {
        &self.value
    }
}

impl<T: ?Sized> Drop for ScopedClosure<'_, T> {
    fn drop(&mut self) {
        if let crate::closure::CallbackOwnership::Owned = self.callback {
            self.value.js_ref().dispose_js_rust_function();
        }
    }
}

impl<T: ?Sized> Unpin for ScopedClosure<'_, T> {}

/// Marker for closures that are safe to invoke through panic-catching glue.
#[doc(hidden)]
pub trait MaybeUnwindSafe {}

impl<T: ?Sized> MaybeUnwindSafe for T {}

/// A trait for converting a Rust closure into the JS closure type `T`.
#[doc(hidden)]
pub trait IntoWasmClosure<T: ?Sized> {
    fn into_closure(self) -> Closure<T>
    where
        Self: Sized,
    {
        unreachable!("unsized closure objects must be converted from Box<Self>")
    }

    fn into_closure_box(self: Box<Self>) -> Closure<T>;
}

/// A trait for converting a shared borrowed Rust closure into the JS closure type `T`.
#[doc(hidden)]
pub trait IntoWasmClosureRef<T: ?Sized> {
    fn into_scoped_closure_ref<'a>(t: &'a Self) -> ScopedClosure<'a, T::Static>
    where
        T: WasmClosure;
}

/// A trait for converting a mutably borrowed Rust closure into the JS closure type `T`.
#[doc(hidden)]
pub trait IntoWasmClosureRefMut<T: ?Sized> {
    fn into_scoped_closure_ref_mut<'a>(t: &'a mut Self) -> ScopedClosure<'a, T::Static>
    where
        T: WasmClosure;
}

/// A trait for converting an `FnOnce(A...) -> R` into a `Closure<dyn FnMut(A...) -> R>`.
#[doc(hidden)]
pub trait WasmClosureFnOnce<T: ?Sized, A, R>: Sized + 'static {
    fn into_closure(self) -> Closure<T>;
}

/// A trait for converting an aborting `FnOnce(A...) -> R` into a closure.
#[doc(hidden)]
pub trait WasmClosureFnOnceAbort<T: ?Sized, A, R>: Sized + 'static {
    fn into_closure(self) -> Closure<T>;
}

impl<T: ?Sized> AsRef<JsValue> for ScopedClosure<'_, T> {
    fn as_ref(&self) -> &JsValue {
        &self.value
    }
}

impl<T> core::fmt::Debug for ScopedClosure<'_, T>
where
    T: ?Sized,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Mirror wasm-bindgen's opaque `Closure { ... }`.
        f.write_str("Closure { ... }")
    }
}

/// Upstream-compatible marker trait for closure signatures.
#[doc(hidden)]
pub trait WasmClosure {
    /// The `'static` version of this closure type.
    type Static: ?Sized;
    /// The mutable version of this closure type.
    type AsMut: ?Sized;
}

/// Internal trait for closure types that can be wrapped and passed to JavaScript.
#[doc(hidden)]
pub trait WryWasmClosure<M> {
    /// Create a Closure from a boxed closure.
    fn into_js_closure(boxed: Box<Self>) -> Closure<Self>;
}

impl<T> ScopedClosure<'static, T>
where
    T: ?Sized + WasmClosure,
{
    pub fn new<F>(t: F) -> Self
    where
        F: IntoWasmClosure<T> + MaybeUnwindSafe + 'static,
    {
        Self::own(t)
    }

    pub fn own<F>(t: F) -> Self
    where
        F: IntoWasmClosure<T> + MaybeUnwindSafe + 'static,
    {
        <F as IntoWasmClosure<T>>::into_closure(t)
    }

    pub fn own_aborting<F>(t: F) -> Self
    where
        F: IntoWasmClosure<T> + 'static,
    {
        <F as IntoWasmClosure<T>>::into_closure(t)
    }

    pub fn own_assert_unwind_safe<F>(t: F) -> Self
    where
        F: IntoWasmClosure<T> + 'static,
    {
        <F as IntoWasmClosure<T>>::into_closure(t)
    }

    pub fn wrap<F>(data: Box<F>) -> Self
    where
        F: IntoWasmClosure<T> + ?Sized + MaybeUnwindSafe,
    {
        <F as IntoWasmClosure<T>>::into_closure_box(data)
    }

    pub fn wrap_aborting<F>(data: Box<F>) -> Self
    where
        F: IntoWasmClosure<T> + ?Sized,
    {
        <F as IntoWasmClosure<T>>::into_closure_box(data)
    }

    pub fn wrap_assert_unwind_safe<F>(data: Box<F>) -> Self
    where
        F: IntoWasmClosure<T> + ?Sized,
    {
        <F as IntoWasmClosure<T>>::into_closure_box(data)
    }

    pub fn borrow<'a, F>(t: &'a F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRef<T> + MaybeUnwindSafe + ?Sized,
    {
        F::into_scoped_closure_ref(t)
    }

    pub fn borrow_aborting<'a, F>(t: &'a F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRef<T> + ?Sized,
    {
        F::into_scoped_closure_ref(t)
    }

    pub fn borrow_assert_unwind_safe<'a, F>(t: &'a F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRef<T> + ?Sized,
    {
        F::into_scoped_closure_ref(t)
    }

    pub fn borrow_mut<'a, F>(t: &'a mut F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRefMut<T> + MaybeUnwindSafe + ?Sized,
    {
        F::into_scoped_closure_ref_mut(t)
    }

    pub fn borrow_mut_aborting<'a, F>(t: &'a mut F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRefMut<T> + ?Sized,
    {
        F::into_scoped_closure_ref_mut(t)
    }

    pub fn borrow_mut_assert_unwind_safe<'a, F>(t: &'a mut F) -> ScopedClosure<'a, T::Static>
    where
        F: IntoWasmClosureRefMut<T> + ?Sized,
    {
        F::into_scoped_closure_ref_mut(t)
    }

    /// Create a `Closure` from a function that can only be called once.
    ///
    /// Since we have no way of enforcing that JS cannot attempt to call this
    /// `FnOnce` more than once, this produces a `Closure<dyn FnMut(A...) -> R>`
    /// that will panic if called more than once.
    pub fn once<F, A, R>(fn_once: F) -> Self
    where
        F: WasmClosureFnOnce<T, A, R> + MaybeUnwindSafe,
    {
        <F as WasmClosureFnOnce<T, A, R>>::into_closure(fn_once)
    }

    pub fn once_aborting<F, A, R>(fn_once: F) -> Self
    where
        F: WasmClosureFnOnceAbort<T, A, R>,
    {
        <F as WasmClosureFnOnceAbort<T, A, R>>::into_closure(fn_once)
    }

    pub fn once_assert_unwind_safe<F, A, R>(fn_once: F) -> Self
    where
        F: WasmClosureFnOnceAbort<T, A, R>,
    {
        Self::once_aborting(fn_once)
    }

    /// Forgets the closure, leaking it.
    pub fn forget(self) {
        core::mem::forget(self);
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
    pub fn once_into_js<F, A, R>(fn_once: F) -> JsValue
    where
        F: WasmClosureFnOnce<T, A, R> + MaybeUnwindSafe,
    {
        Self::once(fn_once).into_js_value()
    }
}

#[cfg(test)]
mod tests {
    use core::panic::{RefUnwindSafe, UnwindSafe};

    use super::ScopedClosure;

    fn assert_auto_traits<T: Send + Sync + UnwindSafe + RefUnwindSafe>() {}

    #[test]
    fn scoped_closure_preserves_auto_traits() {
        assert_auto_traits::<
            ScopedClosure<'static, dyn Fn() + Send + Sync + UnwindSafe + RefUnwindSafe>,
        >();
        assert_auto_traits::<
            ScopedClosure<'static, dyn FnMut() + Send + Sync + UnwindSafe + RefUnwindSafe>,
        >();
    }
}
