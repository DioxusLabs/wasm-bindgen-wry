//! Conversion traits for wasm-bindgen API compatibility.
//!
//! These traits provide compatibility with code that uses wasm-bindgen's
//! low-level ABI conversion types.

use crate::JsValue;

/// Trait for converting a Rust type to its WebAssembly ABI representation.
///
/// In wry-bindgen, this is simplified since we don't use the WebAssembly ABI directly.
/// Instead, we use heap indices (u64) for JsValue references.
pub trait IntoWasmAbi {
    /// The ABI type that this converts into.
    type Abi;

    /// Convert this value into its ABI representation.
    fn into_abi(self) -> Self::Abi;
}

/// Trait for converting from a WebAssembly ABI representation to a Rust type.
///
/// # Safety
/// The caller must ensure the ABI value is valid for the target type.
pub trait FromWasmAbi {
    /// The ABI type that this converts from.
    type Abi;

    /// Convert from the ABI representation to this type.
    ///
    /// # Safety
    /// The caller must ensure the ABI value is valid.
    unsafe fn from_abi(js: Self::Abi) -> Self;
}

// JsValue uses u32 as its ABI type for wasm-bindgen compatibility
// (internally we use u64, but the ABI layer uses u32 for compatibility)
impl IntoWasmAbi for JsValue {
    type Abi = u32;

    fn into_abi(self) -> Self::Abi {
        let id = self.id();
        core::mem::forget(self); // Don't drop - ownership transferred
        id as u32
    }
}

impl FromWasmAbi for JsValue {
    type Abi = u32;

    unsafe fn from_abi(js: Self::Abi) -> Self {
        JsValue::from_id(js as u64)
    }
}

// Implement for reference types
impl IntoWasmAbi for &JsValue {
    type Abi = u32;

    fn into_abi(self) -> Self::Abi {
        self.id() as u32
    }
}

// Implement for Option<JsValue>
impl IntoWasmAbi for Option<JsValue> {
    type Abi = u32;

    fn into_abi(self) -> Self::Abi {
        match self {
            Some(val) => val.into_abi(),
            None => 0, // Use 0 as sentinel for None
        }
    }
}

use crate::ipc::{DecodeError, DecodedData};
use crate::encode::BinaryDecode;
use crate::JsCast;
use core::marker::PhantomData;

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

    fn ref_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        let value = <JsValue as BinaryDecode>::decode(decoder)?;
        Ok(JsCastAnchor {
            value,
            _marker: PhantomData,
        })
    }
}
