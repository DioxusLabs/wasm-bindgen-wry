#![doc = include_str!("../README.md")]
#![no_std]

// The stable runtime-support boundary that `wry-bindgen` builds on. This crate
// exposes only *semantic* operations: typed JS function handles whose `.call()`
// performs a round-trip, a typed object store keyed by an opaque `ObjectHandle`,
// typed thread-local JS values, and the generated-code registration specs. Every
// wire mechanism (function ids, the cached type-id protocol, borrow-stack ids,
// placeholder ids, and `Box<dyn Any>` plumbing) is kept private here so the
// frequently-changing `wry-bindgen-runtime` implementation can evolve beneath
// this boundary without churning `wry-bindgen`'s public surface.

extern crate alloc;

mod batchable;
mod call;
mod callback;
mod clamped;
mod runtime;
mod thread_local;

pub use batchable::{BatchableResult, RequireFlush};
pub use call::JsFunction;
pub use callback::CallbackKey;
pub use clamped::Clamped;
pub use runtime::{Runtime, with_runtime};
pub use thread_local::{JsThreadLocal, LazyCell};

pub use wry_bindgen_runtime::wire::{
    BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, EncodedData,
    JsClassMemberKind, JsClassMemberSpec, JsClassSpec, JsExportSpec, JsFreeExportSpec,
    JsFunctionSpec, JsModuleSpec, JsReexportSpec, JsRef, JsRefEncode, MutSliceArg,
    ObjectBorrowError, ObjectHandle, ObjectRef, ObjectRefMut, ObjectTakeError, RefFromBinaryDecode,
    RustCallback, ThrowingResult, TypeDef, link_to_raw_specifier, register_linked_module,
};
