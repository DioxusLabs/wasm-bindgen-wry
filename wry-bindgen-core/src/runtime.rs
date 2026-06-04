//! The `Runtime` handle and its accessor.
//!
//! `Runtime` exposes borrow-safe runtime operations as methods. Operations that
//! must run code with the runtime released — calling a JS function
//! ([`JsFunction`](crate::JsFunction)), borrowing a stored object
//! ([`Runtime::object`]), or dropping a value — are deliberately not methods here.
//!
//! The handle wraps the concrete [`wry_bindgen_runtime::wire::Runtime`] but only
//! re-exposes the borrow-safe, semantic subset of its operations.

use alloc::boxed::Box;
use core::ops::{Deref, DerefMut};

use wry_bindgen_runtime::wire::Runtime as RuntimeState;
use wry_bindgen_runtime::wire::{DecodedData, EncodedData, JsRef, ObjectHandle};

use crate::BatchableResult;

/// An active borrow of an object from the runtime.
///
/// The object is reinserted when this value is dropped unless a drop for the
/// handle was requested during the checkout.
struct CheckedOutObject<T: 'static> {
    handle: ObjectHandle,
    value: Option<T>,
}

impl<T: 'static> CheckedOutObject<T> {
    /// Check the object out using the already-borrowed runtime. `object()` is
    /// itself called from inside a [`with_runtime`] closure, so this must not
    /// re-enter `with_runtime` (that would be a double borrow).
    fn checkout(rt: &mut Runtime, handle: ObjectHandle) -> Self {
        let value = rt.take_object::<T>(handle).expect("invalid handle");
        Self {
            handle,
            value: Some(value),
        }
    }

    fn get(&self) -> &T {
        self.value.as_ref().expect("checked-out object missing")
    }

    fn get_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("checked-out object missing")
    }
}

impl<T> Deref for CheckedOutObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> DerefMut for CheckedOutObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T: 'static> Drop for CheckedOutObject<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            with_runtime(|rt| rt.reinsert_object(self.handle, value));
        }
    }
}

/// A handle to the active runtime, scoped to a [`with_runtime`] call.
pub struct Runtime<'a> {
    backend: &'a mut RuntimeState,
}

impl Runtime<'_> {
    /// Store a Rust value, returning an opaque handle to it.
    pub fn insert_object<T: 'static>(&mut self, obj: T) -> ObjectHandle {
        self.backend.insert_object_box(Box::new(obj))
    }

    fn take_object<T: 'static>(&mut self, handle: ObjectHandle) -> Option<T> {
        self.backend
            .take_object_box(handle)
            .map(|obj| *obj.downcast::<T>().expect("object type mismatch"))
    }

    /// Temporarily borrow a stored object by handle.
    pub fn object<T: 'static>(
        &mut self,
        handle: ObjectHandle,
    ) -> impl DerefMut<Target = T> + use<T> {
        CheckedOutObject::checkout(self, handle)
    }

    /// Permanently remove a stored value by handle, freeing the handle.
    pub fn remove_object<T: 'static>(&mut self, handle: ObjectHandle) -> Option<T> {
        self.backend
            .remove_object_untyped(handle)
            .map(|obj| *obj.downcast::<T>().expect("object type mismatch"))
    }

    fn reinsert_object<T: 'static>(&mut self, handle: ObjectHandle, obj: T) {
        self.backend.reinsert_object_box(handle, Box::new(obj));
    }

    /// Reserve the next return-value placeholder JS reference.
    ///
    /// Batched calls reserve the heap slot here so the typed result can be
    /// produced without a round-trip; JS fills the slot on the next flush.
    pub(crate) fn next_placeholder_ref(&mut self) -> JsRef {
        self.backend.next_placeholder_ref()
    }
}

/// Run `f` with the active runtime.
///
/// Panics if no runtime is installed, or if one is already borrowed (e.g. a
/// re-entrant call from inside another `with_runtime`).
pub fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    wry_bindgen_runtime::wire::with_runtime(|backend| f(&mut Runtime { backend }))
}

pub(crate) fn with_backend<R>(f: impl FnOnce(&mut RuntimeState) -> R) -> R {
    wry_bindgen_runtime::wire::with_runtime(f)
}

/// Call a JS function synchronously and decode its typed result.
///
/// The function id, placeholder reservation, and flush are all internal to the
/// runtime; this wrapper supplies the typed encode/decode for `R`.
pub(crate) fn run_js_sync<R: BatchableResult + 'static>(
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData),
) -> R {
    wry_bindgen_runtime::wire::run_js_sync(
        fn_id,
        add_args,
        |backend| R::try_placeholder(&mut Runtime { backend }),
        |mut data: DecodedData<'_>| R::decode(&mut data).expect("Failed to decode return value"),
    )
}
