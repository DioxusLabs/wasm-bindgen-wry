//! Value-lifetime extension methods on opaque JS and Rust object handles.

use wry_bindgen_abi::{JsRef, ObjectHandle};

/// Lifetime operations on a JS heap object.
pub trait JsRefExt {
    /// Release this JS heap object.
    fn drop_js_object(self);
}

impl JsRefExt for JsRef {
    fn drop_js_object(self) {
        wry_bindgen_runtime::drop_js_object(self);
    }
}

/// Lifetime operations on a stored Rust object.
pub trait ObjectHandleExt {
    /// Release this stored Rust object.
    fn drop_rust_object(self);
}

impl ObjectHandleExt for ObjectHandle {
    fn drop_rust_object(self) {
        wry_bindgen_runtime::drop_rust_object(self);
    }
}
