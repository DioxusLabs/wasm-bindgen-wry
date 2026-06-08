//! Conversion traits for wasm-bindgen API compatibility.
//!
//! These traits provide compatibility with code that uses wasm-bindgen's
//! low-level ABI conversion types.

use crate::__rt::{
    BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, EncodedData, JsRef,
    MutSliceArg,
};
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

// `IntoWasmAbi`/`FromWasmAbi` are implemented per-type, mirroring upstream
// wasm-bindgen, rather than through a single blanket over `BinaryEncode`. This
// keeps `Result` outside `IntoWasmAbi`, which is what lets `ReturnWasmAbi` carve
// out `Result` for throwing returns (see below) without a coherence conflict.
//
// Every JS heap value is `JsGeneric`, which already requires the wire traits, so
// one blanket covers js-sys/web-sys and every generated `extern` type.
impl<T: JsGeneric> IntoWasmAbi for T {}
impl<T: JsGeneric> FromWasmAbi for T {}

// The value types that are not `JsGeneric` are enumerated explicitly.
macro_rules! value_wasm_abi {
    ($($ty:ty),* $(,)?) => {$(
        impl IntoWasmAbi for $ty {}
        impl FromWasmAbi for $ty {}
    )*};
}
value_wasm_abi!(
    (),
    bool,
    char,
    f32,
    f64,
    usize,
    isize,
    alloc::string::String,
    i8,
    i16,
    i32,
    i64,
    i128,
    u8,
    u16,
    u32,
    u64,
    u128,
);

// Value types upcast to themselves (identity) and widen to `JsValue`, so a
// closure that yields/accepts a value type can be viewed through the wider JS
// type (e.g. `dyn Fn() -> i32` -> `dyn Fn() -> JsValue`). The wire encoding is
// unchanged — a primitive already rides the boundary as its JS value — so these
// markers only authorize the type-level reinterpretation.
macro_rules! value_upcast {
    ($($ty:ty),* $(,)?) => {$(
        impl UpcastFrom<$ty> for $ty {}
        impl UpcastFrom<$ty> for JsValue {}
    )*};
}
value_upcast!(
    bool, char, f32, f64, usize, isize, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128,
);

impl<T: BinaryEncode + EncodeTypeDef> IntoWasmAbi for alloc::vec::Vec<T> {}
impl<T: BinaryDecode + EncodeTypeDef> FromWasmAbi for alloc::vec::Vec<T> {}
impl<T: BinaryEncode + EncodeTypeDef> IntoWasmAbi for core::option::Option<T> {}
impl<T: BinaryDecode + EncodeTypeDef> FromWasmAbi for core::option::Option<T> {}
impl<T: BinaryEncode + EncodeTypeDef> IntoWasmAbi for alloc::boxed::Box<[T]> {}
impl<T: BinaryDecode + EncodeTypeDef> FromWasmAbi for alloc::boxed::Box<[T]> {}

// A shared `&JsValue` flows across the boundary by id, matching wasm-bindgen's
// `IntoWasmAbi for &JsValue`.
impl IntoWasmAbi for &JsValue {}

impl<T> OptionIntoWasmAbi for T where T: IntoWasmAbi {}
impl<T> OptionFromWasmAbi for T where T: FromWasmAbi {}
impl<T: ?Sized> WasmAbi for T {}
impl<T: ?Sized> RefFromWasmAbi for T {}

/// Converts the return value of an exported function into wire bytes. Mirrors
/// wasm-bindgen's `ReturnWasmAbi`: a blanket implementation forwards every
/// `IntoWasmAbi` value directly, while `Result` is carved out so its `Err` is
/// thrown in JS. Because `Result` is not `IntoWasmAbi`, the two implementations
/// do not overlap, and dispatch is by type (so it sees through type aliases).
pub trait ReturnWasmAbi {
    /// The type whose `TypeDef` is advertised to JS for this return value.
    type Wire: EncodeTypeDef;

    /// Encode `self` as the function's return payload.
    fn return_abi(self, encoder: &mut crate::__rt::EncodedData);
}

impl<T: IntoWasmAbi> ReturnWasmAbi for T {
    type Wire = T;

    #[inline]
    fn return_abi(self, encoder: &mut crate::__rt::EncodedData) {
        self.encode(encoder);
    }
}

impl<T, E> ReturnWasmAbi for Result<T, E>
where
    T: BinaryEncode + EncodeTypeDef,
    E: Into<JsValue>,
{
    type Wire = crate::__rt::ThrowingResult<T, JsValue>;

    #[inline]
    fn return_abi(self, encoder: &mut crate::__rt::EncodedData) {
        crate::__rt::ThrowingResult(self.map_err(Into::into)).encode(encoder);
    }
}

/// Converts a `JsValue` into a Rust type by checking at runtime.
pub trait TryFromJsValue: Sized {
    fn try_from_js_value(value: JsValue) -> Result<Self, JsValue> {
        Self::try_from_js_value_ref(&value).ok_or(value)
    }

    fn try_from_js_value_ref(value: &JsValue) -> Option<Self>;
}

/// Lowers the output of an exported `async fn` to the `Result<JsValue, JsValue>`
/// that backs a JS promise (an `Err` becomes a rejected promise). Mirrors
/// wasm-bindgen's `IntoJsResult`, with an added `Resolution` associated type so
/// the macro can advertise the `Promise<…>` wire type. `Result` is carved out by
/// type — seen through aliases — and does not overlap the blanket because
/// `Result` is not `Into<JsValue>`.
pub trait IntoJsResult {
    /// The resolution type of the `Promise` this return value produces.
    type Resolution;

    fn into_js_result(self) -> Result<JsValue, JsValue>;
}

impl<T: Into<JsValue> + crate::sys::Promising> IntoJsResult for T {
    type Resolution = <T as crate::sys::Promising>::Resolution;

    #[inline]
    fn into_js_result(self) -> Result<JsValue, JsValue> {
        Ok(self.into())
    }
}

impl<T: Into<JsValue> + crate::sys::Promising, E: Into<JsValue>> IntoJsResult for Result<T, E> {
    type Resolution = <T as crate::sys::Promising>::Resolution;

    #[inline]
    fn into_js_result(self) -> Result<JsValue, JsValue> {
        match self {
            Ok(value) => Ok(value.into()),
            Err(error) => Err(error.into()),
        }
    }
}

/// Reconstructs the declared return type of an `async` import from the
/// `Result<JsValue, JsValue>` a settled JS promise yields. A `Result<T, E>`
/// return propagates a rejection as `Err`; any other return type panics on
/// rejection. `Result` is dispatched by type, so it is seen through aliases.
pub trait FromJsFuture: Sized {
    fn from_js_future(result: Result<JsValue, JsValue>) -> Self;
}

impl<T: TryFromJsValue> FromJsFuture for T {
    #[inline]
    fn from_js_future(result: Result<JsValue, JsValue>) -> Self {
        let value = result.expect("async function failed");
        T::try_from_js_value(value).expect("async function returned incompatible value")
    }
}

impl<T: TryFromJsValue, E: From<JsValue>> FromJsFuture for Result<T, E> {
    #[inline]
    fn from_js_future(result: Result<JsValue, JsValue>) -> Self {
        match result {
            Ok(value) => Ok(
                T::try_from_js_value(value).expect("async function returned incompatible value")
            ),
            Err(error) => Err(E::from(error)),
        }
    }
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

// A `ScopedClosure` upcasts wherever its underlying closure type does — return
// covariance and argument contravariance both reduce to an upcast of the inner
// `dyn Fn(..)`/`dyn FnMut(..)` signature (e.g. `dyn Fn() -> i32` ->
// `dyn Fn() -> Number`). All `ScopedClosure`s erase to the same repr, so the
// pointer cast in `Upcast::upcast` is sound.
impl<'a, T: ?Sized, U: ?Sized> UpcastFrom<crate::ScopedClosure<'a, U>>
    for crate::ScopedClosure<'a, T>
where
    T: UpcastFrom<U>,
{
}

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

// `RefFromBinaryDecode` is defined in the runtime (so its closure-encode impls
// can borrow-decode a first argument) and re-exported here; the impls for JS
// handles (below) and exported structs (generated) live in this crate.
pub use crate::__rt::RefFromBinaryDecode;

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

impl<T: JsCast> JsCastAnchor<T> {
    /// Anchor the next borrowed reference JS pushed onto its borrow stack. A
    /// borrowed arg arrives without a heap id, so the value is taken by position
    /// from the runtime's borrow stack.
    #[doc(hidden)]
    pub fn next_borrowed() -> Self {
        JsCastAnchor {
            value: JsValue::from_ref(JsRef::next_borrowed_ref()),
            _marker: PhantomData,
        }
    }
}

// `RefFromBinaryDecode` is implemented per JS-handle type (not via a blanket over
// `JsCast`): the trait now lives in the runtime, so a blanket `impl<T: JsCast>`
// here would violate the orphan rule. Imported `extern` types get this impl from
// codegen; `JsValue` is the hand-written base case.
impl RefFromBinaryDecode for JsValue {
    type Wire = RefArg<JsValue>;
    type Anchor = JsCastAnchor<JsValue>;

    fn ref_decode(_decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        Ok(JsCastAnchor::next_borrowed())
    }
}

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

/// Decode a `&mut T` export argument. An exported struct is borrowed *mutably*
/// from the store (an exclusive borrow that composes with the receiver's borrow,
/// so aliasing the receiver reports "recursive use of an object" rather than
/// silently consuming it); a JS-handle type is decoded owned and lent `&mut`.
pub trait BorrowMutArg {
    /// The wire type JS sees for this argument.
    type Wire: EncodeTypeDef;

    /// The anchor that keeps the decoded `&mut Self` valid for the call's duration.
    type Anchor: core::ops::DerefMut<Target = Self>;

    fn borrow_mut_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError>;

    /// Append any post-call write-back data to the export response.
    fn write_back(_anchor: Self::Anchor, _encoder: &mut EncodedData) {}
}

impl<T> core::ops::DerefMut for OwnedArgAnchor<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl BorrowMutArg for JsValue {
    type Wire = RefMutArg<JsValue>;
    type Anchor = OwnedArgAnchor<JsValue>;

    fn borrow_mut_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        Ok(OwnedArgAnchor {
            value: JsValue::decode(decoder)?,
        })
    }
}

impl<T> BorrowMutArg for [T]
where
    T: BinaryDecode + BinaryEncode + EncodeTypeDef,
{
    type Wire = MutSliceArg<T>;
    type Anchor = MutSliceArg<T>;

    fn borrow_mut_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        MutSliceArg::decode(decoder)
    }

    fn write_back(anchor: Self::Anchor, encoder: &mut EncodedData) {
        MutSliceArg::write_back(anchor, encoder);
    }
}
