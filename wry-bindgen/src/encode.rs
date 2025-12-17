//! Core encoding and decoding traits for the binary protocol.
//!
//! This module provides traits for serializing and deserializing Rust types
//! to/from the binary IPC protocol.

use crate::batch::BatchState;
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use crate::value::JsValue;
use std::marker::PhantomData;

/// Trait for creating a JavaScript type instance.
/// Used to map Rust types to their JavaScript type constructors.
pub trait TypeConstructor<P = ()> {
    fn create_type_instance() -> String;
}

/// Trait for encoding Rust values into the binary protocol.
/// Each type specifies how to serialize itself.
pub trait BinaryEncode<P = ()> {
    fn encode(self, encoder: &mut EncodedData);
}

/// Trait for decoding values from the binary protocol.
/// Each type specifies how to deserialize itself.
pub trait BinaryDecode: Sized {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError>;
}

/// Trait for return types that can be used in batched JS calls.
/// Determines how the type behaves during batching.
pub trait BatchableResult: BinaryDecode + std::fmt::Debug {
    /// Whether this result type requires flushing the batch to get the actual value.
    /// Returns false for opaque types (placeholder) and trivial types (known value).
    fn needs_flush() -> bool;

    /// Get a placeholder/trivial value during batching.
    /// For opaque types, this reserves a heap ID from the batch.
    /// For trivial types like (), this returns the known value.
    /// For types that need_flush, this is never called.
    fn batched_placeholder(batch: &mut BatchState) -> Self;
}

// Unit type implementations

impl BatchableResult for () {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(_: &mut BatchState) -> Self {}
}

impl TypeConstructor for () {
    fn create_type_instance() -> String {
        "new window.NullType()".to_string()
    }
}

impl BinaryEncode for () {
    fn encode(self, _encoder: &mut EncodedData) {
        // Unit type encodes as nothing
    }
}

impl BinaryDecode for () {
    fn decode(_decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(())
    }
}

// Boolean implementations

impl TypeConstructor for bool {
    fn create_type_instance() -> String {
        "new window.BoolType()".to_string()
    }
}

impl BinaryEncode for bool {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u8(if self { 1 } else { 0 });
    }
}

impl BinaryDecode for bool {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u8()? != 0)
    }
}

// u8 implementations

impl TypeConstructor for u8 {
    fn create_type_instance() -> String {
        "window.U8Type".to_string()
    }
}

impl BinaryEncode for u8 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u8(self);
    }
}

impl BinaryDecode for u8 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        decoder.take_u8()
    }
}

// u16 implementations

impl TypeConstructor for u16 {
    fn create_type_instance() -> String {
        "window.U16Type".to_string()
    }
}

impl BinaryEncode for u16 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u16(self);
    }
}

impl BinaryDecode for u16 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        decoder.take_u16()
    }
}

// u32 implementations

impl TypeConstructor for u32 {
    fn create_type_instance() -> String {
        "window.U32Type".to_string()
    }
}

impl BinaryEncode for u32 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self);
    }
}

impl BinaryDecode for u32 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        decoder.take_u32()
    }
}

// u64 implementations

impl BinaryEncode for u64 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self);
    }
}

impl BinaryDecode for u64 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        decoder.take_u64()
    }
}

// String/str implementations

impl TypeConstructor for str {
    fn create_type_instance() -> String {
        "window.strType".to_string()
    }
}

impl BinaryEncode for &str {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(self);
    }
}

impl TypeConstructor for String {
    fn create_type_instance() -> String {
        <str as TypeConstructor>::create_type_instance()
    }
}

impl BinaryEncode for String {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(&self);
    }
}

impl BinaryDecode for String {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_str()?.to_string())
    }
}

// Option implementations

impl<T: TypeConstructor<P>, P> TypeConstructor<P> for Option<T> {
    fn create_type_instance() -> String {
        format!("new window.OptionType({})", T::create_type_instance())
    }
}

impl<T: BinaryDecode> BinaryDecode for Option<T> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let has_value = decoder.take_u8()? != 0;
        if has_value {
            Ok(Some(T::decode(decoder)?))
        } else {
            Ok(None)
        }
    }
}

impl<T: BinaryDecode + std::fmt::Debug> BatchableResult for Option<T> {
    fn needs_flush() -> bool {
        // We need to read the response to know if it's Some or None
        true
    }

    fn batched_placeholder(_batch: &mut BatchState) -> Self {
        unreachable!("needs_flush types should never call batched_placeholder")
    }
}

// JsValue implementations

impl TypeConstructor for JsValue {
    fn create_type_instance() -> String {
        "new window.HeapRefType()".to_string()
    }
}

impl BinaryEncode for JsValue {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.id());
    }
}

impl BinaryEncode for &JsValue {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.id());
    }
}

impl BinaryDecode for JsValue {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(JsValue::from_id(decoder.take_u64()?))
    }
}

impl BatchableResult for JsValue {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(batch: &mut BatchState) -> Self {
        JsValue::from_id(batch.get_next_heap_id())
    }
}

/// Implement BatchableResult for types that always need a flush to get the result.
macro_rules! impl_needs_flush {
    ($($ty:ty),*) => {
        $(
            impl BatchableResult for $ty {
                fn needs_flush() -> bool {
                    true
                }

                fn batched_placeholder(_batch: &mut BatchState) -> Self {
                    unreachable!("needs_flush types should never call batched_placeholder")
                }
            }
        )*
    };
}

impl_needs_flush!(bool, u8, u16, u32, u64, String);

/// Marker type for Rust callback parameter types.
pub struct RustCallbackMarker<P> {
    phantom: PhantomData<P>,
}
