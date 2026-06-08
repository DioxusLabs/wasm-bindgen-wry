//! The unstable runtime seam shared between the Rust and JavaScript sides:
//! binary encode/decode, the opaque value/object handles, the IPC buffers, and
//! the generated-code registration specs — together with the runtime handle
//! (`Runtime`), its accessor (`with_runtime`), the batching controls (`batch`,
//! `batch_async`, `force_flush`), and the synchronous call primitive
//! (`run_js_sync`) that drive them.
//!
//! These types were previously their own `wry-bindgen-abi` crate. They live
//! here because the encoding paths run directly against the active runtime.
//! `wry-bindgen-core` re-exports the subset `wry-bindgen` names to build its
//! stable, semantic boundary, and `wry-launch` uses the batching controls to
//! drive the host event loop. Raw id access and the spec accessors stay
//! `pub(crate)`, so they are reachable across the runtime crate but never
//! escape to those consumers.

mod callback;
mod encode;
mod ipc;
mod js_ref;
mod object_store;
mod registry;

pub use callback::{IntoRustCallback, RustCallback};
pub use encode::{
    BinaryDecode, BinaryEncode, EncodeTypeDef, FunctionTypeInfo, JsRefEncode, MutSliceArg,
    RefFromBinaryDecode, ThrowingResult, TypeDef,
};
pub use ipc::{DecodeError, DecodedData, EncodedData};
pub use js_ref::JsRef;
pub use object_store::ObjectHandle;
pub use registry::{
    JsClassMemberKind, JsClassMemberSpec, JsClassSpec, JsExportSpec, JsFreeExportSpec,
    JsFunctionSpec, JsModuleSpec, JsReexportSpec,
};

/// The runtime side of `link_to!`: register a JS snippet (or resolve a raw
/// module specifier) at runtime, returning the URL the WebView fetches it from.
/// The macro-expanded `link_to!` call invokes these through `wry-bindgen`'s
/// `__rt` module.
pub use crate::function_registry::{link_to_raw_specifier, register_linked_module};

/// The runtime handle, its accessor, the batching controls, and the
/// synchronous JS-call primitive. Their orchestration drives the
/// operation-frame lifecycle in [`batch`](crate::batch), so it lives there;
/// they are surfaced here as the runtime half of the wire seam that
/// `wry-bindgen-core` and `wry-launch` build on.
pub use crate::batch::{
    ObjectBorrowError, ObjectRef, ObjectRefMut, ObjectTakeError, Runtime, batch, batch_async,
    force_flush, run_js_sync, with_runtime,
};
