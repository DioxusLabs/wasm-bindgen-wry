//! This is an internal module, no stability guarantees are provided. Use at
//! your own risk.

#![doc(hidden)]

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::panic::AssertUnwindSafe;
use core::{mem::MaybeUninit, ptr::NonNull};

use cfg_if::cfg_if;

use crate::{__rt::marker::ErasableGeneric, JsError, JsValue};
use wry_bindgen_core::Clamped;

macro_rules! tys {
    ($($name:ident)*) => {
        tys! { @ 0; $($name)* }
    };
    (@ $value:expr;) => {};
    (@ $value:expr; $name:ident $($rest:ident)*) => {
        pub const $name: u32 = $value;
        tys! { @ $value + 1; $($rest)* }
    };
}

tys! {
    I8
    U8
    I16
    U16
    I32
    U32
    I64
    U64
    I64_AS_F64
    U64_AS_F64
    I128
    U128
    F32
    F64
    BOOLEAN
    FUNCTION
    CLOSURE
    CACHED_STRING
    STRING
    REF
    REFMUT
    LONGREF
    SLICE
    VECTOR
    EXTERNREF
    NAMED_EXTERNREF
    ENUM
    STRING_ENUM
    DYNAMIC_UNION
    RUST_STRUCT
    CHAR
    OPTIONAL
    RESULT
    UNIT
    CLAMPED
    NONNULL
    RAW_POINTER
}

#[inline(always)]
pub fn inform(_: u32) {}

pub trait WasmDescribe {
    fn describe();
}

/// Trait for element types to implement WasmDescribe for vectors of
/// themselves.
pub trait WasmDescribeVector {
    fn describe_vector();
}

macro_rules! simple {
    ($($t:ident => $d:ident)*) => ($(
        impl WasmDescribe for $t {
            fn describe() {
                inform($d);
            }
        }
    )*)
}

simple! {
    i8 => I8
    u8 => U8
    i16 => I16
    u16 => U16
    i32 => I32
    u32 => U32
    i64 => I64
    u64 => U64
    i128 => I128
    u128 => U128
    f32 => F32
    f64 => F64
    bool => BOOLEAN
    char => CHAR
    JsValue => EXTERNREF
}

cfg_if! {
    if #[cfg(target_arch = "wasm64")] {
        simple! {
            isize => I64_AS_F64
            usize => U64_AS_F64
        }
    } else {
        simple! {
            isize => I32
            usize => U32
        }
    }
}

cfg_if! {
    if #[cfg(feature = "enable-interning")] {
        simple! {
            str => CACHED_STRING
        }
    } else {
        simple! {
            str => STRING
        }
    }
}

impl<T> WasmDescribe for *const T {
    fn describe() {
        inform(RAW_POINTER);
    }
}

impl<T> WasmDescribe for *mut T {
    fn describe() {
        inform(RAW_POINTER);
    }
}

impl<T> WasmDescribe for NonNull<T> {
    fn describe() {
        inform(NONNULL);
    }
}

impl<T: WasmDescribe> WasmDescribe for [T] {
    fn describe() {
        inform(SLICE);
        T::describe();
    }
}

impl<T: WasmDescribe + ?Sized> WasmDescribe for &T {
    fn describe() {
        inform(REF);
        T::describe();
    }
}

impl<T: WasmDescribe + ?Sized> WasmDescribe for &mut T {
    fn describe() {
        inform(REFMUT);
        T::describe();
    }
}

cfg_if! {
    if #[cfg(feature = "enable-interning")] {
        simple! {
            String => CACHED_STRING
        }
    } else {
        simple! {
            String => STRING
        }
    }
}

impl<T: ErasableGeneric<Repr = JsValue> + WasmDescribe> WasmDescribeVector for T {
    fn describe_vector() {
        inform(VECTOR);
        T::describe();
    }
}

impl<T: WasmDescribeVector> WasmDescribe for Box<[T]> {
    fn describe() {
        T::describe_vector();
    }
}

impl<T> WasmDescribe for Vec<T>
where
    Box<[T]>: WasmDescribe,
{
    fn describe() {
        <Box<[T]>>::describe();
    }
}

impl<T: WasmDescribe> WasmDescribe for Option<T> {
    fn describe() {
        inform(OPTIONAL);
        T::describe();
    }
}

impl WasmDescribe for () {
    fn describe() {
        inform(UNIT);
    }
}

impl<T: WasmDescribe, E: Into<JsValue>> WasmDescribe for Result<T, E> {
    fn describe() {
        inform(RESULT);
        T::describe();
    }
}

impl<T: WasmDescribe> WasmDescribe for MaybeUninit<T> {
    fn describe() {
        T::describe();
    }
}

impl<T: WasmDescribe> WasmDescribe for Clamped<T> {
    fn describe() {
        inform(CLAMPED);
        T::describe();
    }
}

impl WasmDescribe for JsError {
    fn describe() {
        JsValue::describe();
    }
}

impl<T> WasmDescribe for AssertUnwindSafe<T>
where
    T: WasmDescribe,
{
    fn describe() {
        T::describe();
    }
}
