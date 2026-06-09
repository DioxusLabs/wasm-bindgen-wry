use crate::encode::{
    Anchored, BinaryDecode, BinaryEncode, BorrowScope, CallScoped, EncodeTypeDef, JsRef,
    ThrowingResult,
};
use crate::ipc::EncodedData;
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

/// The wire type advertised to JS for a return value, in borrow scope `S` — the
/// return-side analog of [`ArgAbi<S>::Wire`](crate::convert::ArgAbi::Wire). For
/// [`CallScoped`] it is the value's own wire type; for [`Anchored`] it is the
/// `Promise` *resolution* (the export macro wraps it in the configured
/// `js_sys::Promise<…>`, since `Promise` lives in the external `js-sys` crate).
/// The encode/lower behavior lives on the [`ReturnSync`]/[`ReturnAsync`]
/// sub-traits, so a value implements only the scope(s) it is returnable in.
pub trait ReturnAbi<S: BorrowScope> {
    /// The type whose `TypeDef` is advertised to JS for this return value.
    type Wire: EncodeTypeDef;
}

/// Encode a synchronous export's return value as wire bytes. A blanket forwards
/// every `IntoWasmAbi` value directly; `Result` is carved out so its `Err` is
/// thrown in JS. Because `Result` is not `IntoWasmAbi` the two do not overlap,
/// and dispatch is by type (so it sees through type aliases).
pub trait ReturnSync: ReturnAbi<CallScoped> {
    /// Encode `self` as the function's return payload.
    fn return_abi(self, encoder: &mut EncodedData);
}

impl<T: IntoWasmAbi> ReturnAbi<CallScoped> for T {
    type Wire = T;
}
impl<T: IntoWasmAbi> ReturnSync for T {
    #[inline]
    fn return_abi(self, encoder: &mut EncodedData) {
        self.encode(encoder);
    }
}

impl<T, E> ReturnAbi<CallScoped> for Result<T, E>
where
    T: BinaryEncode + EncodeTypeDef,
    E: Into<JsValue>,
{
    type Wire = ThrowingResult<T, JsValue>;
}
impl<T, E> ReturnSync for Result<T, E>
where
    T: BinaryEncode + EncodeTypeDef,
    E: Into<JsValue>,
{
    #[inline]
    fn return_abi(self, encoder: &mut EncodedData) {
        ThrowingResult(self.map_err(Into::into)).encode(encoder);
    }
}

// An exported constructor hands JS the stored object's handle by value (JS then
// `__wrap`s it). `ObjectHandle` is not `IntoWasmAbi` and is only ever returned by
// a *sync* constructor, so it implements the sync scope only.
impl ReturnAbi<CallScoped> for crate::__rt::object_store::ObjectHandle {
    type Wire = Self;
}
impl ReturnSync for crate::__rt::object_store::ObjectHandle {
    #[inline]
    fn return_abi(self, encoder: &mut EncodedData) {
        self.encode(encoder);
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
/// that backs a JS promise (an `Err` becomes a rejected promise). A blanket
/// covers every `Into<JsValue> + Promising` value; `Result` is carved out by type
/// (it does not overlap because `Result` is not `Into<JsValue>`). The promise
/// resolution type is [`ReturnAbi<Anchored>::Wire`], delegated to [`Promising`].
pub trait ReturnAsync: ReturnAbi<Anchored> {
    fn into_js_result(self) -> Result<JsValue, JsValue>;
}

impl<T> ReturnAbi<Anchored> for T
where
    T: Into<JsValue> + crate::sys::Promising,
    <T as crate::sys::Promising>::Resolution: EncodeTypeDef,
{
    type Wire = <T as crate::sys::Promising>::Resolution;
}
impl<T> ReturnAsync for T
where
    T: Into<JsValue> + crate::sys::Promising,
    <T as crate::sys::Promising>::Resolution: EncodeTypeDef,
{
    #[inline]
    fn into_js_result(self) -> Result<JsValue, JsValue> {
        Ok(self.into())
    }
}

impl<T, E> ReturnAbi<Anchored> for Result<T, E>
where
    T: Into<JsValue> + crate::sys::Promising,
    <T as crate::sys::Promising>::Resolution: EncodeTypeDef,
    E: Into<JsValue>,
{
    type Wire = <T as crate::sys::Promising>::Resolution;
}
impl<T, E> ReturnAsync for Result<T, E>
where
    T: Into<JsValue> + crate::sys::Promising,
    <T as crate::sys::Promising>::Resolution: EncodeTypeDef,
    E: Into<JsValue>,
{
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
