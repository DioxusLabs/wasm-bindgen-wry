//! The `Runtime` handle and its accessor.
//!
//! `Runtime` exposes borrow-safe runtime operations as methods. Operations that
//! must run code with the runtime released — calling a JS function
//! ([`JsFunction`](crate::JsFunction)) or dropping a value — are deliberately
//! not methods here.
//!
//! The handle wraps the concrete [`wry_bindgen_runtime::wire::Runtime`] but only
//! re-exposes the borrow-safe, semantic subset of its operations.

use wry_bindgen_runtime::wire::Runtime as RuntimeState;
use wry_bindgen_runtime::wire::{
    DecodedData, EncodedData, JsRef, ObjectBorrowError, ObjectHandle, ObjectRef, ObjectRefMut,
    ObjectTakeError,
};

use crate::BatchableResult;

/// A handle to the active runtime, scoped to a [`with_runtime`] call.
pub struct Runtime<'a> {
    backend: &'a mut RuntimeState,
}

impl Runtime<'_> {
    /// Store a Rust value, returning an opaque handle to it.
    pub fn insert_object<T: 'static>(&mut self, obj: T) -> ObjectHandle {
        self.backend.insert_object_box(alloc::boxed::Box::new(obj))
    }

    /// Temporarily shared-borrow a stored object by handle.
    pub fn object_ref<T: 'static>(
        &mut self,
        handle: ObjectHandle,
    ) -> Result<ObjectRef<T>, ObjectBorrowError> {
        self.backend.object_ref(handle)
    }

    /// Temporarily mutable-borrow a stored object by handle.
    pub fn object_mut<T: 'static>(
        &mut self,
        handle: ObjectHandle,
    ) -> Result<ObjectRefMut<T>, ObjectBorrowError> {
        self.backend.object_mut(handle)
    }

    /// Whether the object stored at `handle` is of type `T`, without removing
    /// it. Used by fallible casts that must not consume the value on a type
    /// mismatch.
    pub fn object_is<T: 'static>(&self, handle: ObjectHandle) -> bool {
        self.backend.object_is::<T>(handle)
    }

    /// Permanently remove a stored value by handle, freeing the handle.
    pub fn remove_object<T: 'static>(
        &mut self,
        handle: ObjectHandle,
    ) -> Result<T, ObjectTakeError> {
        self.backend.remove_object_untyped(handle).and_then(|obj| {
            obj.downcast::<T>()
                .map(|obj| *obj)
                .map_err(|_| ObjectTakeError::TypeMismatch)
        })
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
        |data: &mut DecodedData<'_>| R::decode(data).expect("Failed to decode return value"),
    )
}
