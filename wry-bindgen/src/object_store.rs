//! Object-store helpers used by generated code, built on `wry-bindgen-core`.
//!
//! These wrappers adapt the core object-store operations and the local JS-wrapper
//! constructor for the generated `__rt` surface.

use wry_bindgen_core::{ObjectHandleExt, with_runtime};

pub use wry_bindgen_core::ObjectHandle;

/// Remove and drop a stored object by handle.
pub fn drop_object(handle: ObjectHandle) {
    handle.drop_rust_object()
}

pub fn with_object<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&T) -> R) -> R {
    let obj = with_runtime(|rt| rt.object::<T>(handle));
    f(&obj)
}

pub fn with_object_mut<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&mut T) -> R) -> R {
    let mut obj = with_runtime(|rt| rt.object::<T>(handle));
    f(&mut obj)
}

pub fn insert_object<T: 'static>(obj: T) -> ObjectHandle {
    with_runtime(|rt| rt.insert_object(obj))
}

pub fn remove_object<T: 'static>(handle: ObjectHandle) -> T {
    with_runtime(|rt| rt.remove_object::<T>(handle).expect("invalid handle"))
}

/// Create a JavaScript wrapper object for an exported Rust struct.
pub fn create_js_wrapper(handle: ObjectHandle, class_name: &str) -> crate::JsValue {
    crate::js_helpers::create_rust_object_wrapper(handle, class_name)
}
