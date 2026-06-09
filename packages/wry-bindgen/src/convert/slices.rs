use crate::__rt::{BinaryDecode, BinaryEncode, EncodeTypeDef};
use core::ops::Deref;

use super::{FromWasmAbi, IntoWasmAbi};

impl<T: BinaryEncode + EncodeTypeDef> IntoWasmAbi for alloc::vec::Vec<T> {}
impl<T: BinaryDecode + EncodeTypeDef> FromWasmAbi for alloc::vec::Vec<T> {}
impl<T: BinaryEncode + EncodeTypeDef> IntoWasmAbi for alloc::boxed::Box<[T]> {}
impl<T: BinaryDecode + EncodeTypeDef> FromWasmAbi for alloc::boxed::Box<[T]> {}

/// Position marker for a `&T` argument's wire type. A by-value `T` advertises its
/// own type def (an exported struct's `RustValue`, an imported type's `HeapRef`).
/// A borrowed JS-handle value rides the borrow stack as `BorrowedRef`, while an
/// exported struct borrow rides the wire as `RustBorrow(class_name)` so JS can
/// route inheritance descendants to their ancestor view.
pub struct RefArg<T: ?Sized>(core::marker::PhantomData<T>);

impl<T: EncodeTypeDef + ?Sized> EncodeTypeDef for RefArg<T> {
    fn encode_type_def(type_def: &mut crate::__rt::TypeDef) {
        match crate::__rt::TypeDef::rust_value_class_name::<T>() {
            Some(class_name) => type_def.rust_borrow(&class_name),
            None => type_def.borrowed_ref(),
        }
    }
}

/// Anchor holding an owned JS-handle value by reference. A heap-ref decode does
/// not consume the underlying JS object, so holding the owned wrapper and lending
/// `&T` matches wasm-bindgen's borrowed-handle semantics.
pub struct OwnedArgAnchor<T> {
    value: T,
}

impl<T> OwnedArgAnchor<T> {
    #[doc(hidden)]
    pub fn from_value(value: T) -> Self {
        OwnedArgAnchor { value }
    }
}

impl<T> Deref for OwnedArgAnchor<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> core::ops::DerefMut for OwnedArgAnchor<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Position marker for a `&mut T` argument's wire type. Like [`RefArg`], an
/// exported struct rides the wire as a routed handle (`RustBorrow`) so the call
/// borrows it from the store rather than consuming it; a JS-handle type keeps its
/// owned-decode path.
pub struct RefMutArg<T: ?Sized>(core::marker::PhantomData<T>);

impl<T: EncodeTypeDef + ?Sized> EncodeTypeDef for RefMutArg<T> {
    fn encode_type_def(type_def: &mut crate::__rt::TypeDef) {
        match crate::__rt::TypeDef::rust_value_class_name::<T>() {
            Some(class_name) => type_def.rust_borrow(&class_name),
            None => T::encode_type_def(type_def),
        }
    }
}
