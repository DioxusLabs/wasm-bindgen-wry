//! Object store for exported Rust structs
//!
//! This module provides the runtime infrastructure for storing Rust objects
//! that are exported to JavaScript. Objects are stored by handle (u32) and
//! can be retrieved, borrowed, and dropped.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::any::Any;
use core::cell::{Ref, RefCell, RefMut};

/// Handle to an exported object in the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectHandle(pub u32);

impl ObjectHandle {
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Thread-local store for exported Rust objects.
pub struct ObjectStore {
    objects: BTreeMap<u32, Box<dyn Any>>,
    next_handle: u32,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            next_handle: 1,
        }
    }

    pub fn insert<T: 'static>(&mut self, obj: T) -> ObjectHandle {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        if self.next_handle == 0 {
            self.next_handle = 1;
        }
        self.objects.insert(handle, Box::new(RefCell::new(obj)));
        ObjectHandle(handle)
    }

    pub fn get<T: 'static>(&self, handle: ObjectHandle) -> Ref<'_, T> {
        let boxed = self.objects.get(&handle.0).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow()
    }

    pub fn get_mut<T: 'static>(&self, handle: ObjectHandle) -> RefMut<'_, T> {
        let boxed = self.objects.get(&handle.0).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow_mut()
    }

    pub fn remove<T: 'static>(&mut self, handle: ObjectHandle) -> T {
        let boxed = self.objects.remove(&handle.0).expect("invalid handle");
        let cell = boxed.downcast::<RefCell<T>>().expect("type mismatch");
        cell.into_inner()
    }

    pub fn remove_untyped(&mut self, handle: ObjectHandle) -> bool {
        self.objects.remove(&handle.0).is_some()
    }
}

impl Default for ObjectStore {
    fn default() -> Self {
        Self::new()
    }
}

std::thread_local! {
    pub static OBJECT_STORE: RefCell<ObjectStore> = RefCell::new(ObjectStore::new());
}

pub fn with_object<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&T) -> R) -> R {
    OBJECT_STORE.with(|store| {
        let store = store.borrow();
        let obj = store.get::<T>(handle);
        f(&*obj)
    })
}

pub fn with_object_mut<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&mut T) -> R) -> R {
    OBJECT_STORE.with(|store| {
        let store = store.borrow();
        let mut obj = store.get_mut::<T>(handle);
        f(&mut *obj)
    })
}

pub fn insert_object<T: 'static>(obj: T) -> ObjectHandle {
    OBJECT_STORE.with(|store| store.borrow_mut().insert(obj))
}

pub fn remove_object<T: 'static>(handle: ObjectHandle) -> T {
    OBJECT_STORE.with(|store| store.borrow_mut().remove(handle))
}

pub fn drop_object(handle: ObjectHandle) -> bool {
    OBJECT_STORE.with(|store| store.borrow_mut().remove_untyped(handle))
}
