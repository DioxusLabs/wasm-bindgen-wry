#![doc = include_str!("../README.md")]
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
mod wry;

/// The unstable runtime seam: the wire vocabulary (binary encode/decode, the
/// opaque value/object handles, the IPC buffers, the generated-code
/// registration specs) plus the runtime handle, its accessor, the batching
/// controls, and the synchronous call primitive that drive it.
///
/// **Unstable.** Published for `wry-bindgen-core` (which wraps it in a stable,
/// semantic boundary) and `wry-launch` (the host event-loop integration).
/// Nothing here is covered by semver — application code should depend on
/// `wry-bindgen-core` instead.
#[doc(hidden)]
pub mod wire;

pub use wry::{
    ProtocolHandler, WryBindgen, WryBindgenDriver, WryBindgenRuntime, WryBindgenWebviewDriver,
};

mod encode {
    pub(crate) use crate::wire::BinaryDecode;
}

mod function {
    pub(crate) const DROP_NATIVE_REF_FN_ID: u32 = 0xFFFF_FFFF;
    pub(crate) const CALL_EXPORT_FN_ID: u32 = 0xFFFF_FFFE;
    pub(crate) use crate::wire::RustCallback;
}

mod object_store {
    use alloc::boxed::Box;
    use core::any::Any;

    pub(crate) use crate::wire::ObjectHandle;

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
