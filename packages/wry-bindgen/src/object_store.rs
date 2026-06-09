//! Object-store helpers used by generated code, built on `wry-bindgen-core`.

use alloc::rc::Rc;
use core::cell::{Ref, RefMut};
use ouroboros::self_referencing;

use wry_bindgen_core::{ObjectBorrowError, ObjectRef, ObjectRefMut, ObjectTakeError, with_runtime};

use crate::Parent;

pub use wry_bindgen_core::ObjectHandle;

const RECURSIVE_USE: &str = "recursive use of an object";

type ParentBorrow<'a, T> = Ref<'a, T>;
type ParentBorrowMut<'a, T> = RefMut<'a, T>;

#[self_referencing(no_doc)]
struct ParentRefBorrowCell<T: 'static> {
    cell: Rc<core::cell::RefCell<T>>,

    #[borrows(cell)]
    #[covariant]
    borrow: ParentBorrow<'this, T>,
}

#[self_referencing(no_doc)]
struct ParentMutBorrowCell<T: 'static> {
    cell: Rc<core::cell::RefCell<T>>,

    #[borrows(cell)]
    #[not_covariant]
    borrow: ParentBorrowMut<'this, T>,
}

/// Remove and drop a stored object by handle.
pub fn drop_object(handle: ObjectHandle) {
    handle.drop_rust_object()
}

pub fn with_object<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&T) -> R) -> R {
    let guard = checkout_object_ref::<T>(handle);
    f(&guard)
}

pub fn with_object_mut<T: 'static, R>(handle: ObjectHandle, f: impl FnOnce(&mut T) -> R) -> R {
    let mut guard = checkout_object_mut::<T>(handle);
    f(&mut guard)
}

pub fn checkout_object_ref<T: 'static>(
    handle: ObjectHandle,
) -> impl core::ops::Deref<Target = T> + 'static {
    SharedCheckout::open(handle)
}

pub fn checkout_object_mut<T: 'static>(
    handle: ObjectHandle,
) -> impl core::ops::DerefMut<Target = T> + 'static {
    CheckoutGuard::open(handle)
}

enum SharedCheckout<T: 'static> {
    Owned(ObjectRef<T>),
    Ancestor(ParentRefBorrow<T>),
}

impl<T: 'static> SharedCheckout<T> {
    fn open(handle: ObjectHandle) -> Self {
        if object_is::<Parent<T>>(handle) {
            SharedCheckout::Ancestor(ParentRefBorrow::open(handle))
        } else {
            SharedCheckout::Owned(
                with_runtime(|rt| rt.object_ref::<T>(handle))
                    .unwrap_or_else(|error| throw_borrow_error(error)),
            )
        }
    }
}

impl<T: 'static> core::ops::Deref for SharedCheckout<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            SharedCheckout::Owned(guard) => guard,
            SharedCheckout::Ancestor(guard) => guard,
        }
    }
}

enum CheckoutGuard<T: 'static> {
    Owned(ObjectRefMut<T>),
    Ancestor(ParentMutBorrow<T>),
}

impl<T: 'static> CheckoutGuard<T> {
    fn open(handle: ObjectHandle) -> Self {
        if object_is::<Parent<T>>(handle) {
            CheckoutGuard::Ancestor(ParentMutBorrow::open(handle))
        } else {
            CheckoutGuard::Owned(
                with_runtime(|rt| rt.object_mut::<T>(handle))
                    .unwrap_or_else(|error| throw_borrow_error(error)),
            )
        }
    }
}

impl<T: 'static> core::ops::Deref for CheckoutGuard<T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            CheckoutGuard::Owned(guard) => guard,
            CheckoutGuard::Ancestor(guard) => guard,
        }
    }
}

impl<T: 'static> core::ops::DerefMut for CheckoutGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            CheckoutGuard::Owned(guard) => guard,
            CheckoutGuard::Ancestor(guard) => guard,
        }
    }
}

struct ParentRefBorrow<T: 'static> {
    inner: ParentRefBorrowCell<T>,
}

impl<T: 'static> ParentRefBorrow<T> {
    fn open(handle: ObjectHandle) -> Self {
        let parent = with_runtime(|rt| rt.object_ref::<Parent<T>>(handle))
            .unwrap_or_else(|error| throw_borrow_error(error));
        let cell = parent.share_cell();
        drop(parent);
        let inner = ParentRefBorrowCellTryBuilder {
            cell,
            borrow_builder: |cell| cell.try_borrow().map_err(|_| ()),
        }
        .try_build()
        .unwrap_or_else(|_| crate::throw_str(RECURSIVE_USE));
        Self { inner }
    }
}

impl<T: 'static> core::ops::Deref for ParentRefBorrow<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.with_borrow(|borrow| &**borrow)
    }
}

struct ParentMutBorrow<T: 'static> {
    inner: ParentMutBorrowCell<T>,
}

impl<T: 'static> ParentMutBorrow<T> {
    fn open(handle: ObjectHandle) -> Self {
        let parent = with_runtime(|rt| rt.object_ref::<Parent<T>>(handle))
            .unwrap_or_else(|error| throw_borrow_error(error));
        let cell = parent.share_cell();
        drop(parent);
        let inner = ParentMutBorrowCellTryBuilder {
            cell,
            borrow_builder: |cell| cell.try_borrow_mut().map_err(|_| ()),
        }
        .try_build()
        .unwrap_or_else(|_| crate::throw_str(RECURSIVE_USE));
        Self { inner }
    }
}

impl<T: 'static> core::ops::Deref for ParentMutBorrow<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.with_borrow(|borrow| &**borrow)
    }
}

impl<T: 'static> core::ops::DerefMut for ParentMutBorrow<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.with_borrow_mut(|borrow| &mut **borrow)
    }
}

pub fn insert_object<T: 'static>(obj: T) -> ObjectHandle {
    with_runtime(|rt| rt.insert_object(obj))
}

pub fn remove_object<T: 'static>(handle: ObjectHandle) -> T {
    with_runtime(|rt| rt.remove_object::<T>(handle)).unwrap_or_else(|error| throw_take_error(error))
}

/// Whether the object stored at `handle` is of type `T`, without removing it.
pub fn object_is<T: 'static>(handle: ObjectHandle) -> bool {
    with_runtime(|rt| rt.object_is::<T>(handle))
}

fn throw_borrow_error(error: ObjectBorrowError) -> ! {
    match error {
        ObjectBorrowError::InvalidHandle => crate::throw_str("null pointer passed to rust"),
        ObjectBorrowError::RecursiveUse => crate::throw_str(RECURSIVE_USE),
        ObjectBorrowError::TypeMismatch => crate::throw_str("object type mismatch"),
    }
}

fn throw_take_error(error: ObjectTakeError) -> ! {
    match error {
        ObjectTakeError::InvalidHandle => crate::throw_str("null pointer passed to rust"),
        ObjectTakeError::Borrowed => crate::throw_str(error.message()),
        ObjectTakeError::TypeMismatch => crate::throw_str("object type mismatch"),
    }
}

/// Validate that the object behind a borrowed `&T`/`&mut T` argument really is a
/// `T` (its own value or an ancestor view of it), before any borrow is opened.
fn expect_class<T: 'static + crate::__rt::EncodeTypeDef>(handle: ObjectHandle) {
    if object_is::<T>(handle) || object_is::<Parent<T>>(handle) {
        return;
    }
    match crate::__rt::TypeDef::rust_value_class_name::<T>() {
        Some(class_name) => crate::throw_str(&alloc::format!("expected instance of {class_name}")),
        None => crate::throw_str("expected instance of a Rust exported class"),
    }
}

/// Create a JavaScript wrapper object for an exported Rust struct.
pub fn create_js_wrapper(handle: ObjectHandle, class_name: &str) -> crate::JsValue {
    crate::js_helpers::create_rust_object_wrapper(handle, class_name)
}

/// Anchors an exported struct borrowed as a `&T` argument.
pub struct ObjectRefAnchor<T: 'static> {
    guard: SharedCheckout<T>,
}

impl<T: 'static> core::ops::Deref for ObjectRefAnchor<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: 'static + crate::__rt::EncodeTypeDef> ObjectRefAnchor<T> {
    #[doc(hidden)]
    pub fn checkout_borrowed() -> crate::__rt::core::result::Result<Self, crate::__rt::DecodeError>
    {
        let wrapper = crate::JsValue::from_ref(crate::__rt::JsRef::next_borrowed_ref());
        let handle = crate::__rt::extract_rust_handle(&wrapper).ok_or_else(|| {
            crate::__rt::DecodeError::custom(
                "expected a Rust object wrapper for a borrowed callback argument",
            )
        })?;
        expect_class::<T>(handle);
        Ok(ObjectRefAnchor {
            guard: SharedCheckout::open(handle),
        })
    }

    #[doc(hidden)]
    pub fn checkout_from_decoder(
        decoder: &mut crate::__rt::DecodedData,
    ) -> crate::__rt::core::result::Result<Self, crate::__rt::DecodeError> {
        let handle = <ObjectHandle as crate::__rt::BinaryDecode>::decode(decoder)?;
        expect_class::<T>(handle);
        Ok(ObjectRefAnchor {
            guard: SharedCheckout::open(handle),
        })
    }
}

/// Anchors an exported struct borrowed as a `&mut T` argument.
pub struct ObjectRefMutAnchor<T: 'static> {
    guard: CheckoutGuard<T>,
}

impl<T: 'static> core::ops::Deref for ObjectRefMutAnchor<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: 'static> core::ops::DerefMut for ObjectRefMutAnchor<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T: 'static + crate::__rt::EncodeTypeDef> ObjectRefMutAnchor<T> {
    #[doc(hidden)]
    pub fn checkout_from_decoder(
        decoder: &mut crate::__rt::DecodedData,
    ) -> crate::__rt::core::result::Result<Self, crate::__rt::DecodeError> {
        let handle = <ObjectHandle as crate::__rt::BinaryDecode>::decode(decoder)?;
        expect_class::<T>(handle);
        Ok(ObjectRefMutAnchor {
            guard: CheckoutGuard::open(handle),
        })
    }
}
