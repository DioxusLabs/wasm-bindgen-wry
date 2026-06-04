//! Wry runtime transport for `wry-bindgen`.

#![no_std]

extern crate alloc;
#[macro_use]
extern crate std;

mod batch;
mod function_registry;
mod id_allocator;
mod ipc;
mod js_helpers;
mod runtime;
mod type_cache;
pub mod wry;

pub use batch::{batch, batch_async, force_flush};

/// The runtime-support seam consumed by `wry-bindgen-core`. These expose the
/// active runtime and its operations directly; `wry-bindgen-core` wraps them in
/// semantic handles, so they never reach the stable public surface.
pub use batch::{
    Runtime, queue_rust_object_drop, run_js_sync, try_queue_js_dispose_rust_function,
    try_queue_js_drop, try_queue_rust_object_drop, with_runtime,
};
pub use wry::{
    ProtocolHandler, WryBindgen, WryBindgenDriver, WryBindgenRuntime, WryBindgenWebviewDriver,
};
pub use wry_bindgen_abi::BinaryDecode;

mod encode {
    pub use wry_bindgen_abi::BinaryDecode;
}

mod function {
    pub(crate) const DROP_NATIVE_REF_FN_ID: u32 = 0xFFFF_FFFF;
    pub(crate) const CALL_EXPORT_FN_ID: u32 = 0xFFFF_FFFE;
    pub use wry_bindgen_abi::RustCallback;
}

mod object_store {
    use alloc::boxed::Box;
    use core::any::Any;

    pub use wry_bindgen_abi::ObjectHandle;

    pub(crate) fn drop_object(handle: ObjectHandle) -> bool {
        let object: Option<Box<dyn Any>> =
            crate::batch::with_runtime(|state| state.remove_object_untyped(handle));
        let dropped = object.is_some();
        drop(object);
        dropped
    }
}

mod value {
    pub(crate) const JSIDX_OFFSET: u64 = 128;
    pub(crate) const JSIDX_RESERVED: u64 = JSIDX_OFFSET + 4;
}
