//! Shared parent storage for generated extended Rust types.

use crate::__rt::{Ref, RefMut};

/// Storage wrapper for the auto-injected parent field on extended Rust types.
pub struct Parent<T> {
    inner: alloc::rc::Rc<core::cell::RefCell<T>>,
}

impl<T> Clone for Parent<T> {
    fn clone(&self) -> Self {
        Self {
            inner: alloc::rc::Rc::clone(&self.inner),
        }
    }
}

// Match upstream wasm-bindgen, whose `Rc<WasmRefCell<T>>`-backed `Parent` opts into the
// unwind-safe markers (`WasmRefCell` implements both). These are safe marker traits.
impl<T> core::panic::UnwindSafe for Parent<T> {}
impl<T> core::panic::RefUnwindSafe for Parent<T> {}

impl<T: core::fmt::Debug> core::fmt::Debug for Parent<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Parent")
            .field(&*self.inner.borrow())
            .finish()
    }
}

impl<T> Parent<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: alloc::rc::Rc::new(core::cell::RefCell::new(value)),
        }
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            inner: self.inner.borrow(),
        }
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        RefMut {
            inner: self.inner.borrow_mut(),
        }
    }

    /// Clone the shared backing cell. Used by generated upcast exports to publish
    /// an ancestor view that shares the same `T` as the descendant's parent field.
    #[doc(hidden)]
    pub fn share_cell(&self) -> alloc::rc::Rc<core::cell::RefCell<T>> {
        alloc::rc::Rc::clone(&self.inner)
    }

    /// Reconstruct a `Parent<T>` from a previously [`share_cell`](Self::share_cell)d
    /// backing cell, so an ancestor view stored by handle resolves to the same
    /// shared `T`.
    #[doc(hidden)]
    pub fn from_cell(inner: alloc::rc::Rc<core::cell::RefCell<T>>) -> Self {
        Self { inner }
    }
}

impl<T> From<T> for Parent<T> {
    fn from(value: T) -> Self {
        Parent::new(value)
    }
}
