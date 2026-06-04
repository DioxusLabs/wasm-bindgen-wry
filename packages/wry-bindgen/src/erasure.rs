//! Runtime marker implementations for generic type erasure.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{__rt, JsValue, ScopedClosure};

unsafe impl __rt::marker::ErasableGeneric for JsValue {
    type Repr = JsValue;
}

macro_rules! impl_erasable_generic_self {
    ($($ty:ty),* $(,)?) => {
        $(
            unsafe impl __rt::marker::ErasableGeneric for $ty {
                type Repr = $ty;
            }
        )*
    };
}

impl_erasable_generic_self!(
    (),
    bool,
    char,
    f32,
    f64,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
);

unsafe impl<T: __rt::marker::ErasableGeneric> __rt::marker::ErasableGeneric for Option<T> {
    type Repr = Option<T::Repr>;
}

unsafe impl<T: __rt::marker::ErasableGeneric, E: __rt::marker::ErasableGeneric>
    __rt::marker::ErasableGeneric for Result<T, E>
{
    type Repr = Result<T::Repr, E::Repr>;
}

unsafe impl<T: __rt::marker::ErasableGeneric> __rt::marker::ErasableGeneric for Vec<T> {
    type Repr = Vec<T::Repr>;
}

unsafe impl<T: __rt::marker::ErasableGeneric> __rt::marker::ErasableGeneric for Box<[T]> {
    type Repr = Box<[T::Repr]>;
}

unsafe impl __rt::marker::ErasableGeneric for &str {
    type Repr = &'static str;
}

unsafe impl<T: __rt::marker::ErasableGeneric> __rt::marker::ErasableGeneric for &[T] {
    type Repr = &'static [T::Repr];
}

unsafe impl<T: __rt::marker::ErasableGeneric> __rt::marker::ErasableGeneric for &mut [T] {
    type Repr = &'static mut [T::Repr];
}

unsafe impl<T: ?Sized> __rt::marker::ErasableGeneric for ScopedClosure<'_, T> {
    type Repr = ScopedClosure<'static, dyn FnMut()>;
}

macro_rules! impl_fn_ref_erasable_generic {
    ($($arg:ident),* $(,)?) => {
        unsafe impl<'a, R, $($arg,)*> __rt::marker::ErasableGeneric
            for &'a (dyn Fn($($arg),*) -> R + 'a)
        where
            $($arg: __rt::marker::ErasableGeneric,)*
            R: __rt::marker::ErasableGeneric,
        {
            type Repr = &'static (dyn Fn($($arg::Repr),*) -> R::Repr + 'static);
        }

        unsafe impl<'a, R, $($arg,)*> __rt::marker::ErasableGeneric
            for &'a mut (dyn Fn($($arg),*) -> R + 'a)
        where
            $($arg: __rt::marker::ErasableGeneric,)*
            R: __rt::marker::ErasableGeneric,
        {
            type Repr = &'static mut (dyn Fn($($arg::Repr),*) -> R::Repr + 'static);
        }

        unsafe impl<'a, R, $($arg,)*> __rt::marker::ErasableGeneric
            for &'a (dyn FnMut($($arg),*) -> R + 'a)
        where
            $($arg: __rt::marker::ErasableGeneric,)*
            R: __rt::marker::ErasableGeneric,
        {
            type Repr = &'static (dyn FnMut($($arg::Repr),*) -> R::Repr + 'static);
        }

        unsafe impl<'a, R, $($arg,)*> __rt::marker::ErasableGeneric
            for &'a mut (dyn FnMut($($arg),*) -> R + 'a)
        where
            $($arg: __rt::marker::ErasableGeneric,)*
            R: __rt::marker::ErasableGeneric,
        {
            type Repr = &'static mut (dyn FnMut($($arg::Repr),*) -> R::Repr + 'static);
        }
    };
}

impl_fn_ref_erasable_generic!();
impl_fn_ref_erasable_generic!(A1);
impl_fn_ref_erasable_generic!(A1, A2);
impl_fn_ref_erasable_generic!(A1, A2, A3);
impl_fn_ref_erasable_generic!(A1, A2, A3, A4);
impl_fn_ref_erasable_generic!(A1, A2, A3, A4, A5);
impl_fn_ref_erasable_generic!(A1, A2, A3, A4, A5, A6);
impl_fn_ref_erasable_generic!(A1, A2, A3, A4, A5, A6, A7);
impl_fn_ref_erasable_generic!(A1, A2, A3, A4, A5, A6, A7, A8);
