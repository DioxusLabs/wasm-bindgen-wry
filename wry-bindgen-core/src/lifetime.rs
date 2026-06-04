//! Value-lifetime extension methods on the opaque handle types.
//!
//! These run from `Drop` impls and the generated free/GC paths. The `try_*`
//! methods are non-panicking — if no runtime is installed (teardown) they are a
//! no-op. They queue work synchronously; the drop may re-enter the runtime, so
//! they use the owned accessor rather than holding a [`Runtime`](crate::Runtime)
//! borrow.

use wry_bindgen_abi::{JsRef, ObjectHandle};

use crate::runtime::with_backend;

/// Lifetime operations on a JS heap reference.
pub trait JsRefExt {
    /// Queue this JS heap value to be released.
    fn try_queue_drop(self);
    /// Queue disposal of the Rust function backing this callback reference.
    fn try_queue_dispose_rust_function(self);
}

impl JsRefExt for JsRef {
    fn try_queue_drop(self) {
        wry_bindgen_runtime::try_queue_js_drop(self);
    }

    fn try_queue_dispose_rust_function(self) {
        wry_bindgen_runtime::try_queue_js_dispose_rust_function(self);
    }
}

/// Lifetime operations on a stored-object handle.
pub trait ObjectHandleExt {
    /// Queue this stored Rust object to be dropped.
    fn try_queue_drop(self);
    /// Remove and drop this stored object now.
    fn drop_stored(self);
}

impl ObjectHandleExt for ObjectHandle {
    fn try_queue_drop(self) {
        wry_bindgen_runtime::try_queue_rust_object_drop(self);
    }

    fn drop_stored(self) {
        // Drop runs the object's destructor outside the runtime borrow.
        drop(with_backend(|backend| backend.remove_object_untyped(self)));
    }
}
