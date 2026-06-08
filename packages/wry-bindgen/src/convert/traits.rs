use crate::__rt::{BinaryDecode, BinaryEncode, EncodeTypeDef, JsRef};
use crate::{JsCast, JsValue};
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

/// Decode a shared reference that may live across an exported `async fn`.
///
/// Short-lived `&JsValue` arguments can ride JS's borrow stack, but an async
/// export returns a `Promise` before its future has necessarily finished. Those
/// futures need an anchor that remains valid after the caller's borrow frame is
/// gone.
pub trait LongRefFromBinaryDecode {
    /// The wire type JS sees for this argument.
    type Wire: crate::__rt::EncodeTypeDef;

    /// The anchor that keeps the decoded `&Self` valid for the future.
    type Anchor: core::ops::Deref<Target = Self>;

    fn long_ref_decode(
        decoder: &mut crate::__rt::DecodedData,
    ) -> Result<Self::Anchor, crate::__rt::DecodeError>;
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
/// the macro can advertise the `Promise<...>` wire type. `Result` is carved out
/// by type - seen through aliases - and does not overlap the blanket because
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

/// Marker for type-safe generic upcast relationships.
pub trait UpcastFrom<S: ?Sized> {}

/// Type-safe generic upcast helper.
pub trait Upcast<T: ?Sized> {
    #[inline]
    fn upcast(&self) -> &T
    where
        Self: crate::__rt::marker::ErasableGeneric,
        T: Sized
            + crate::__rt::marker::ErasableGeneric<
                Repr = <Self as crate::__rt::marker::ErasableGeneric>::Repr,
            >,
    {
        unsafe { &*(self as *const Self as *const T) }
    }

    #[inline]
    fn upcast_into(self) -> T
    where
        Self: Sized + crate::__rt::marker::ErasableGeneric,
        T: Sized
            + crate::__rt::marker::ErasableGeneric<
                Repr = <Self as crate::__rt::marker::ErasableGeneric>::Repr,
            >,
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

// `RefFromBinaryDecode` is defined in the runtime (so its closure-encode impls
// can borrow-decode a first argument) and re-exported here; the impls for JS
// handles and exported structs live in this crate/codegen.
pub use crate::__rt::RefFromBinaryDecode;
