//! Object store for exported Rust structs and callback functions.
//!
//! This module provides the runtime infrastructure for storing Rust objects
//! that are exported to JavaScript. Objects are stored by handle (u32) and
//! can be retrieved, borrowed, and dropped. It also stores callback functions
//! that can be called from JavaScript.

use crate::batch::with_runtime;
use crate::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef};

/// Handle to an exported object in the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectHandle(u32);

impl BinaryDecode for ObjectHandle {
    fn decode(decoder: &mut crate::DecodedData) -> Result<Self, crate::DecodeError> {
        let raw = u32::decode(decoder)?;
        Ok(ObjectHandle(raw))
    }
}

impl BinaryEncode for ObjectHandle {
    fn encode(self, encoder: &mut crate::EncodedData) {
        self.0.encode(encoder);
    }
}

impl EncodeTypeDef for ObjectHandle {
    fn encode_type_def(buf: &mut std::vec::Vec<u8>) {
        u32::encode_type_def(buf);
    }
}

impl BatchableResult for ObjectHandle {}

pub fn with_object<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&T) -> R) -> R {
    with_runtime(|state| {
        let obj = state.get_object::<T>(handle.0);
        f(&*obj)
    })
}

pub fn with_object_mut<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&mut T) -> R) -> R {
    with_runtime(|state| {
        let mut obj = state.get_object_mut::<T>(handle.0);
        f(&mut *obj)
    })
}

pub fn insert_object<T: 'static>(obj: T) -> ObjectHandle {
    with_runtime(|state| ObjectHandle(state.insert_object(obj)))
}

pub fn remove_object<T: 'static>(handle: ObjectHandle) -> T {
    with_runtime(|state| state.remove_object(handle.0))
}

pub fn drop_object(handle: ObjectHandle) -> bool {
    with_runtime(|state| state.remove_object_untyped(handle.0))
}

/// Create a JavaScript wrapper object for an exported Rust struct.
/// The wrapper is a JS object with methods that call back into Rust via the export specs.
pub fn create_js_wrapper<T: 'static>(handle: ObjectHandle, class_name: &str) -> crate::JsValue {
    // Call into JavaScript to create the wrapper object
    // The JS side will create an object with the appropriate methods
    crate::js_helpers::create_rust_object_wrapper(handle.0, class_name)
}
