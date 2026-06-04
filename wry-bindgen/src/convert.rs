//! Conversion traits for wasm-bindgen API compatibility.
//!
//! These traits provide compatibility with code that uses wasm-bindgen's
//! low-level ABI conversion types.

use crate::__rt::{BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, JsRef};
use crate::JsValue;
use core::mem::ManuallyDrop;
use core::ops::Deref;

/// Marker for types accepted by wasm-bindgen-shaped APIs that conceptually
/// convert into a Wasm ABI value.
///
/// Wry-bindgen does not use wasm-bindgen's raw ABI transport on desktop; the
/// generated glue uses the binary protocol instead. These traits are kept as
/// markers for `js-sys`/`web-sys` signatures that use wasm-bindgen's unstable
/// conversion traits as bounds.
pub trait IntoWasmAbi: BinaryEncode + EncodeTypeDef {
    #[inline]
    fn into_abi(self) -> u32
    where
        Self: Sized + IntoAbiId,
    {
        self.into_abi_id()
    }
}

/// Marker for types accepted by wasm-bindgen-shaped APIs that conceptually
/// convert from a Wasm ABI value.
pub trait FromWasmAbi: BinaryDecode + EncodeTypeDef {
    /// Recreate a JS-reference-like value from a heap id.
    ///
    /// This is only a compatibility hook for crates that preserve `JsValue`
    /// references through serde or similar adapters. Generated Wry bindings use
    /// the binary protocol instead.
    ///
    /// # Safety
    ///
    /// The caller must pass an id for a live JavaScript heap value that is valid
    /// for `Self`.
    #[inline]
    unsafe fn from_abi(js: u32) -> Self
    where
        Self: Sized + FromAbiId,
    {
        unsafe { Self::from_abi_id(js) }
    }
}

/// Marker for types that may appear as `Option<T>` in wasm-bindgen-shaped APIs.
pub trait OptionIntoWasmAbi: IntoWasmAbi {}

/// Marker for types that may be received as `Option<T>` in wasm-bindgen-shaped APIs.
pub trait OptionFromWasmAbi: FromWasmAbi {}

/// Marker for values that have a wasm-bindgen ABI representation.
pub trait WasmAbi {}

/// Marker for types that can be borrowed from wasm-bindgen-shaped APIs.
pub trait RefFromWasmAbi {
    /// Recreate a non-dropping reference anchor from a heap id.
    ///
    /// # Safety
    ///
    /// The caller must pass an id for a live JavaScript heap value that remains
    /// valid for the returned anchor.
    #[inline]
    unsafe fn ref_from_abi(js: u32) -> AbiRef<Self>
    where
        Self: Sized + FromAbiId,
    {
        AbiRef(ManuallyDrop::new(unsafe { Self::from_abi_id(js) }))
    }
}

/// Non-dropping anchor returned by `RefFromWasmAbi::ref_from_abi`.
pub struct AbiRef<T>(ManuallyDrop<T>);

impl<T> Deref for AbiRef<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> AsRef<T> for AbiRef<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

#[doc(hidden)]
pub trait IntoAbiId {
    fn into_abi_id(self) -> u32;
}

#[doc(hidden)]
pub trait FromAbiId {
    unsafe fn from_abi_id(js: u32) -> Self;
}

impl<T> IntoAbiId for T
where
    T: AsRef<JsValue>,
{
    #[inline]
    fn into_abi_id(self) -> u32 {
        let id = self.as_ref().js_ref().into_abi();
        core::mem::forget(self);
        id
    }
}

impl<T> FromAbiId for T
where
    T: JsCast,
{
    #[inline]
    unsafe fn from_abi_id(js: u32) -> Self {
        T::unchecked_from_js(JsValue::from_ref(JsRef::from_abi(js)))
    }
}

impl<T> IntoWasmAbi for T where T: BinaryEncode + EncodeTypeDef {}
impl<T> FromWasmAbi for T where T: BinaryDecode + EncodeTypeDef {}
impl<T> OptionIntoWasmAbi for T where T: IntoWasmAbi {}
impl<T> OptionFromWasmAbi for T where T: FromWasmAbi {}
impl<T: ?Sized> WasmAbi for T {}
impl<T: ?Sized> RefFromWasmAbi for T {}

/// Converts a `JsValue` into a Rust type by checking at runtime.
pub trait TryFromJsValue: Sized {
    fn try_from_js_value(value: JsValue) -> Result<Self, JsValue> {
        Self::try_from_js_value_ref(&value).ok_or(value)
    }

    fn try_from_js_value_ref(value: &JsValue) -> Option<Self>;
}

use crate::{__rt::marker::ErasableGeneric, JsCast};
use core::marker::PhantomData;

/// Marker for type-safe generic upcast relationships.
pub trait UpcastFrom<S: ?Sized> {}

/// Type-safe generic upcast helper.
pub trait Upcast<T: ?Sized> {
    #[inline]
    fn upcast(&self) -> &T
    where
        Self: ErasableGeneric,
        T: Sized + ErasableGeneric<Repr = <Self as ErasableGeneric>::Repr>,
    {
        unsafe { &*(self as *const Self as *const T) }
    }

    #[inline]
    fn upcast_into(self) -> T
    where
        Self: Sized + ErasableGeneric,
        T: Sized + ErasableGeneric<Repr = <Self as ErasableGeneric>::Repr>,
    {
        unsafe { core::mem::transmute_copy(&core::mem::ManuallyDrop::new(self)) }
    }
}

impl<S, T> Upcast<T> for S
where
    T: UpcastFrom<S> + ?Sized,
    S: ?Sized,
{
}

impl<'a, T, Target> UpcastFrom<&'a T> for &'a Target where Target: UpcastFrom<T> {}
impl<'a, T, Target> UpcastFrom<&'a mut T> for &'a mut Target where Target: UpcastFrom<T> {}

macro_rules! impl_tuple_upcast {
    ([$($ty:ident)+] [$($target:ident)+]) => {
        impl<$($ty,)+ $($target,)+> UpcastFrom<($($ty,)+)> for ($($target,)+)
        where
            $($ty: JsGeneric,)+
            $($target: JsGeneric + UpcastFrom<$ty>,)+
        {
        }

        impl<$($ty,)+ $($target,)+> UpcastFrom<($($ty,)+)> for crate::sys::JsOption<($($target,)+)>
        where
            $($ty: JsGeneric,)+
            $($target: JsGeneric + UpcastFrom<$ty>,)+
        {
        }
    };
}

impl_tuple_upcast!([T1][Target1]);
impl_tuple_upcast!([T1 T2] [Target1 Target2]);
impl_tuple_upcast!([T1 T2 T3] [Target1 Target2 Target3]);
impl_tuple_upcast!([T1 T2 T3 T4] [Target1 Target2 Target3 Target4]);
impl_tuple_upcast!([T1 T2 T3 T4 T5] [Target1 Target2 Target3 Target4 Target5]);
impl_tuple_upcast!([T1 T2 T3 T4 T5 T6] [Target1 Target2 Target3 Target4 Target5 Target6]);
impl_tuple_upcast!([T1 T2 T3 T4 T5 T6 T7] [Target1 Target2 Target3 Target4 Target5 Target6 Target7]);
impl_tuple_upcast!([T1 T2 T3 T4 T5 T6 T7 T8] [Target1 Target2 Target3 Target4 Target5 Target6 Target7 Target8]);

macro_rules! impl_fn_upcasts {
    () => {
        impl_fn_upcasts!(@arities
            [0 []]
            [1 [A1 B1] O1]
            [2 [A1 B1 A2 B2] O2]
            [3 [A1 B1 A2 B2 A3 B3] O3]
            [4 [A1 B1 A2 B2 A3 B3 A4 B4] O4]
            [5 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5] O5]
            [6 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6] O6]
            [7 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6 A7 B7] O7]
            [8 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6 A7 B7 A8 B8] O8]
        );
    };

    (@arities) => {};

    (@arities [$n:tt $args:tt $($opt:ident)?] $([$rest_n:tt $rest_args:tt $($rest_opt:ident)?])*) => {
        impl_fn_upcasts!(@same $args);
        impl_fn_upcasts!(@cross_all $args [] $([$rest_n $rest_args $($rest_opt)?])*);
        impl_fn_upcasts!(@arities $([$rest_n $rest_args $($rest_opt)?])*);
    };

    (@same []) => {
        impl<R1, R2> UpcastFrom<fn() -> R1> for fn() -> R2
        where
            R2: UpcastFrom<R1>,
        {
        }

        impl<'a, R1, R2> UpcastFrom<dyn Fn() -> R1 + 'a> for dyn Fn() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
        {
        }

        impl<'a, R1, R2> UpcastFrom<dyn FnMut() -> R1 + 'a> for dyn FnMut() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
        {
        }
    };

    (@same [$($A1:ident $A2:ident)+]) => {
        impl<R1, R2, $($A1, $A2),+> UpcastFrom<fn($($A1),+) -> R1> for fn($($A2),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2),+> UpcastFrom<dyn Fn($($A1),+) -> R1 + 'a> for dyn Fn($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2),+> UpcastFrom<dyn FnMut($($A1),+) -> R1 + 'a> for dyn FnMut($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }
    };

    (@cross_all $args:tt $opts:tt) => {};

    (@cross_all $args:tt [$($opts:ident)*] [$next_n:tt $next_args:tt $next_opt:ident] $([$rest_n:tt $rest_args:tt $($rest_opt:ident)?])*) => {
        impl_fn_upcasts!(@extend $args [$($opts)* $next_opt]);
        impl_fn_upcasts!(@shrink $args [$($opts)* $next_opt]);
        impl_fn_upcasts!(@cross_all $args [$($opts)* $next_opt] $([$rest_n $rest_args $($rest_opt)?])*);
    };

    (@extend [] [$($O:ident)+]) => {
        impl<R1, R2, $($O),+> UpcastFrom<fn() -> R1> for fn($($O),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn Fn() -> R1 + 'a> for dyn Fn($($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn FnMut() -> R1 + 'a> for dyn FnMut($($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@extend [$($A1:ident $A2:ident)+] [$($O:ident)+]) => {
        impl<R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<fn($($A1),+) -> R1> for fn($($A2,)+ $($O),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn Fn($($A1),+) -> R1 + 'a> for dyn Fn($($A2,)+ $($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn FnMut($($A1),+) -> R1 + 'a> for dyn FnMut($($A2,)+ $($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@shrink [] [$($O:ident)+]) => {
        impl<R1, R2, $($O),+> UpcastFrom<fn($($O),+) -> R1> for fn() -> R2
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn Fn($($O),+) -> R1 + 'a> for dyn Fn() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn FnMut($($O),+) -> R1 + 'a> for dyn FnMut() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@shrink [$($A1:ident $A2:ident)+] [$($O:ident)+]) => {
        impl<R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<fn($($A1,)+ $($O),+) -> R1> for fn($($A2),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn Fn($($A1,)+ $($O),+) -> R1 + 'a> for dyn Fn($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn FnMut($($A1,)+ $($O),+) -> R1 + 'a> for dyn FnMut($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };
}

impl_fn_upcasts!();

/// Convenience bound for JS values whose generic parameters erase to `JsValue`.
pub trait JsGeneric:
    crate::__rt::marker::ErasableGeneric<Repr = JsValue>
    + UpcastFrom<Self>
    + Upcast<Self>
    + Upcast<JsValue>
    + JsCast
    + crate::__rt::JsRefEncode
    + crate::__rt::EncodeTypeDef
    + crate::__rt::BinaryEncode
    + crate::__rt::BinaryDecode
    + crate::__rt::BatchableResult
    + 'static
{
}

impl<T> JsGeneric for T where
    T: crate::__rt::marker::ErasableGeneric<Repr = JsValue>
        + UpcastFrom<T>
        + Upcast<JsValue>
        + JsCast
        + crate::__rt::JsRefEncode
        + crate::__rt::EncodeTypeDef
        + crate::__rt::BinaryEncode
        + crate::__rt::BinaryDecode
        + crate::__rt::BatchableResult
        + 'static
{
}

/// Converts a value into its canonical JS-generic representation.
pub trait IntoJsGeneric {
    type JsCanon: JsGeneric;

    fn to_js(self) -> Self::JsCanon;
}

impl IntoJsGeneric for JsValue {
    type JsCanon = JsValue;

    #[inline]
    fn to_js(self) -> JsValue {
        self
    }
}

impl<T: IntoJsGeneric + Clone> IntoJsGeneric for &T {
    type JsCanon = T::JsCanon;

    #[inline]
    fn to_js(self) -> T::JsCanon {
        self.clone().to_js()
    }
}

impl UpcastFrom<JsValue> for JsValue {}

/// Trait for types that can be decoded as references from binary data.
///
/// This is the wry-bindgen equivalent of wasm-bindgen's `RefFromWasmAbi`.
/// The `Anchor` type holds the decoded value and keeps the reference valid
/// during callback invocation.
pub trait RefFromBinaryDecode {
    /// The anchor type that keeps the decoded reference valid.
    type Anchor: core::ops::Deref<Target = Self>;

    /// Decode a reference anchor from binary data.
    fn ref_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError>;
}

/// Anchor type for JsCast references.
///
/// This holds a `JsValue` and provides a reference to the target type `T`
/// through the `JsCast` trait.
pub struct JsCastAnchor<T: JsCast> {
    value: JsValue,
    _marker: PhantomData<T>,
}

impl<T: JsCast> core::ops::Deref for JsCastAnchor<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        T::unchecked_from_js_ref(&self.value)
    }
}

// Blanket implementation for all JsCast types (including JsValue)
impl<T: JsCast + 'static> RefFromBinaryDecode for T {
    type Anchor = JsCastAnchor<T>;

    fn ref_decode(_decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        // For borrowed refs, we use the borrow stack (indices 1-127) instead of heap IDs.
        // JS puts the value on its borrow stack without sending an ID, so we sync by
        // taking the next borrowed ref from the runtime.
        let value = JsValue::from_ref(JsRef::next_borrowed_ref());
        Ok(JsCastAnchor {
            value,
            _marker: PhantomData,
        })
    }
}
