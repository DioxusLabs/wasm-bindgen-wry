use crate::__rt::{DecodeError, DecodedData, JsRef};
use crate::{JsCast, JsValue};
use core::marker::PhantomData;

use super::{
    FromWasmAbi, IntoWasmAbi, JsGeneric, OptionFromWasmAbi, OptionIntoWasmAbi, RefArg,
    RefFromBinaryDecode, RefFromWasmAbi, UpcastFrom, WasmAbi,
};

// `IntoWasmAbi`/`FromWasmAbi` are implemented per-type, mirroring upstream
// wasm-bindgen, rather than through a single blanket over `BinaryEncode`. This
// keeps `Result` outside `IntoWasmAbi`, which is what lets `ReturnWasmAbi` carve
// out `Result` for throwing returns without a coherence conflict.
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
// unchanged - a primitive already rides the boundary as its JS value - so these
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

impl<T: crate::__rt::BinaryEncode + crate::__rt::EncodeTypeDef> IntoWasmAbi
    for core::option::Option<T>
{
}

impl<T: crate::__rt::BinaryDecode + crate::__rt::EncodeTypeDef> FromWasmAbi
    for core::option::Option<T>
{
}

// A shared `&JsValue` flows across the boundary by id, matching wasm-bindgen's
// `IntoWasmAbi for &JsValue`.
impl IntoWasmAbi for &JsValue {}

impl<T> OptionIntoWasmAbi for T where T: IntoWasmAbi {}
impl<T> OptionFromWasmAbi for T where T: FromWasmAbi {}
impl<T: ?Sized> WasmAbi for T {}
impl<T: ?Sized> RefFromWasmAbi for T {}

impl UpcastFrom<JsValue> for JsValue {}

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
// `JsCast`): the trait lives in the runtime, so a blanket `impl<T: JsCast>` here
// would violate the orphan rule. Imported `extern` types get this impl from
// codegen; `JsValue` is the hand-written base case.
impl RefFromBinaryDecode for JsValue {
    type Wire = RefArg<JsValue>;
    type Anchor = JsCastAnchor<JsValue>;

    fn ref_decode(_decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        Ok(JsCastAnchor::next_borrowed())
    }
}
