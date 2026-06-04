//! The stable runtime-support boundary that `wry-bindgen` builds on.
//!
//! This crate exposes only *semantic* operations: typed JS function handles
//! whose `.call()` performs a round-trip, a typed object store keyed by an
//! opaque [`ObjectHandle`], typed thread-local JS values, value-lifetime
//! helpers, and the generated-code registration specs. Every wire mechanism
//! (function ids, the cached type-id protocol, borrow-stack ids,
//! placeholder ids, and `Box<dyn Any>` plumbing) is kept private here so the
//! frequently-changing `wry-bindgen-runtime` implementation can evolve beneath
//! this boundary without churning `wry-bindgen`'s public surface.

#![no_std]

extern crate alloc;

mod batchable;
mod call;
mod callback;
mod clamped;
mod lifetime;
mod runtime;
mod thread_local;

pub use batchable::{BatchableResult, RequireFlush};
pub use call::JsFunction;
pub use callback::CallbackKey;
pub use clamped::Clamped;
pub use lifetime::{JsRefExt, ObjectHandleExt};
pub use runtime::{Runtime, with_runtime};
pub use thread_local::JsThreadLocal;

pub use wry_bindgen_abi::{
    BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, EncodedData,
    JsClassMemberKind, JsClassMemberSpec, JsExportSpec, JsFunctionSpec, JsModuleSpec, JsRef,
    JsRefEncode, ObjectHandle, RustCallback, TypeDef,
};
