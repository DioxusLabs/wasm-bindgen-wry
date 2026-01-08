//! Object store for exported Rust structs and callback functions.
//!
//! This module provides the runtime infrastructure for storing Rust objects
//! that are exported to JavaScript. Objects are stored by handle (u32) and
//! can be retrieved, borrowed, and dropped. It also stores callback functions
//! that can be called from JavaScript.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::any::Any;
use core::cell::{Ref, RefCell, RefMut};

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

impl BatchableResult for ObjectHandle {
    fn needs_flush() -> bool {
        true
    }

    fn batched_placeholder(_: &mut crate::batch::BatchState) -> Self {
        unreachable!()
    }
}

/// Encoder for storing Rust objects that can be called from JS.
/// Also stores exported Rust structs for the object store.
pub(crate) struct ObjEncoder {
    /// Exported Rust structs stored by handle
    objects: BTreeMap<u32, Box<dyn Any>>,
    /// Next handle to assign for exported objects
    next_handle: u32,
}

impl ObjEncoder {
    pub(crate) fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            next_handle: 1,
        }
    }

    /// Insert an exported object and return its handle.
    pub(crate) fn insert_object<T: 'static>(&mut self, obj: T) -> u32 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        if self.next_handle == 0 {
            self.next_handle = 1;
        }
        self.objects.insert(handle, Box::new(RefCell::new(obj)));
        handle
    }

    /// Get a reference to an exported object.
    pub(crate) fn get_object<T: 'static>(&self, handle: u32) -> Ref<'_, T> {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow()
    }

    /// Get a mutable reference to an exported object.
    pub(crate) fn get_object_mut<T: 'static>(&self, handle: u32) -> RefMut<'_, T> {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow_mut()
    }

    /// Remove an exported object and return it.
    pub(crate) fn remove_object<T: 'static>(&mut self, handle: u32) -> T {
        let boxed = self.objects.remove(&handle).expect("invalid handle");
        let cell = boxed.downcast::<RefCell<T>>().expect("type mismatch");
        cell.into_inner()
    }

    /// Remove an exported object without returning it.
    pub(crate) fn remove_object_untyped(&mut self, handle: u32) -> bool {
        self.objects.remove(&handle).is_some()
    }
}

std::thread_local! {
    pub(crate) static OBJECT_STORE: RefCell<ObjEncoder> = RefCell::new(ObjEncoder::new());
}

pub fn with_object<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&T) -> R) -> R {
    OBJECT_STORE.with(|encoder| {
        let encoder = encoder.borrow();
        let obj: Ref<'_, T> = encoder.get_object(handle.0);
        f(&*obj)
    })
}

pub fn with_object_mut<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&mut T) -> R) -> R {
    OBJECT_STORE.with(|encoder| {
        let encoder = encoder.borrow();
        let mut obj: RefMut<'_, T> = encoder.get_object_mut(handle.0);
        f(&mut *obj)
    })
}

pub fn insert_object<T: 'static>(obj: T) -> ObjectHandle {
    OBJECT_STORE.with(|encoder| {
        ObjectHandle(encoder.borrow_mut().insert_object(obj))
    })
}

pub fn remove_object<T: 'static>(handle: ObjectHandle) -> T {
    OBJECT_STORE.with(|encoder| {
        encoder.borrow_mut().remove_object(handle.0)
    })
}

pub fn drop_object(handle: ObjectHandle) -> bool {
    OBJECT_STORE.with(|encoder| {
        encoder.borrow_mut().remove_object_untyped(handle.0)
    })
}

/// Create a JavaScript wrapper object for an exported Rust struct.
/// The wrapper is a JS object with methods that call back into Rust via the export specs.
pub fn create_js_wrapper<T: 'static>(handle: ObjectHandle, class_name: &str) -> crate::JsValue {
    // Call into JavaScript to create the wrapper object
    // The JS side will create an object with the appropriate methods
    crate::js_helpers::create_rust_object_wrapper(handle.0, class_name)
}
