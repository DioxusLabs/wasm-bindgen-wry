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
pub trait BatchableResult: BinaryDecode {
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

impl TypeConstructor for u64 {
    fn create_type_instance() -> String {
        "window.U64Type".to_string()
    }
}

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

// i8 implementations

impl TypeConstructor for i8 {
    fn create_type_instance() -> String {
        "window.I8Type".to_string()
    }
}

impl BinaryEncode for i8 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u8(self as u8);
    }
}

impl BinaryDecode for i8 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u8()? as i8)
    }
}

// i16 implementations

impl TypeConstructor for i16 {
    fn create_type_instance() -> String {
        "window.I16Type".to_string()
    }
}

impl BinaryEncode for i16 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u16(self as u16);
    }
}

impl BinaryDecode for i16 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u16()? as i16)
    }
}

// i32 implementations

impl TypeConstructor for i32 {
    fn create_type_instance() -> String {
        "window.I32Type".to_string()
    }
}

impl BinaryEncode for i32 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self as u32);
    }
}

impl BinaryDecode for i32 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u32()? as i32)
    }
}

// i64 implementations

impl TypeConstructor for i64 {
    fn create_type_instance() -> String {
        "window.I64Type".to_string()
    }
}

impl BinaryEncode for i64 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self as u64);
    }
}

impl BinaryDecode for i64 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u64()? as i64)
    }
}

// f32 implementations

impl TypeConstructor for f32 {
    fn create_type_instance() -> String {
        "window.F32Type".to_string()
    }
}

impl BinaryEncode for f32 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.to_bits());
    }
}

impl BinaryDecode for f32 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(f32::from_bits(decoder.take_u32()?))
    }
}

// f64 implementations

impl TypeConstructor for f64 {
    fn create_type_instance() -> String {
        "window.F64Type".to_string()
    }
}

impl BinaryEncode for f64 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.to_bits());
    }
}

impl BinaryDecode for f64 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(f64::from_bits(decoder.take_u64()?))
    }
}

// usize implementations (uses u64 for portability)

impl TypeConstructor for usize {
    fn create_type_instance() -> String {
        "window.UsizeType".to_string()
    }
}

impl BinaryEncode for usize {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self as u64);
    }
}

impl BinaryDecode for usize {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u64()? as usize)
    }
}

// isize implementations (uses i64 for portability)

impl TypeConstructor for isize {
    fn create_type_instance() -> String {
        "window.IsizeType".to_string()
    }
}

impl BinaryEncode for isize {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self as u64);
    }
}

impl BinaryDecode for isize {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u64()? as isize)
    }
}

// String/str implementations

impl TypeConstructor for str {
    fn create_type_instance() -> String {
        "window.strType".to_string()
    }
}

// Explicit impl for &str since str is not Sized and blanket impl doesn't apply
impl TypeConstructor for &str {
    fn create_type_instance() -> String {
        <str as TypeConstructor>::create_type_instance()
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

// Encoding for Option<T> where T is encodable
impl<T: BinaryEncode<P>, P> BinaryEncode<P> for Option<T> {
    fn encode(self, encoder: &mut EncodedData) {
        match self {
            Some(val) => {
                encoder.push_u8(1);
                val.encode(encoder);
            }
            None => {
                encoder.push_u8(0);
            }
        }
    }
}

impl<T: BinaryDecode> BatchableResult for Option<T> {
    fn needs_flush() -> bool {
        // We need to read the response to know if it's Some or None
        true
    }

    fn batched_placeholder(_batch: &mut BatchState) -> Self {
        unreachable!("needs_flush types should never call batched_placeholder")
    }
}

// Result implementations

impl<T: TypeConstructor<P>, E: TypeConstructor<P>, P> TypeConstructor<P> for Result<T, E> {
    fn create_type_instance() -> String {
        format!(
            "new window.ResultType({}, {})",
            T::create_type_instance(),
            E::create_type_instance()
        )
    }
}

impl<T: BinaryDecode, E: BinaryDecode> BinaryDecode for Result<T, E> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let is_ok = decoder.take_u8()? != 0;
        if is_ok {
            Ok(Ok(T::decode(decoder)?))
        } else {
            Ok(Err(E::decode(decoder)?))
        }
    }
}

impl<T: BinaryDecode, E: BinaryDecode> BatchableResult for Result<T, E> {
    fn needs_flush() -> bool {
        // We need to read the response to know if it's Ok or Err
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

impl_needs_flush!(
    bool, u8, u16, u32, u64, i8, i16, i32, i64, isize, usize, f32, f64, String
);

// Blanket implementation for references to types that implement BinaryEncode via clone
// Note: We only implement for P=() to avoid conflicts with RustCallbackMarker impls
impl<T: BinaryEncode + Clone> BinaryEncode for &T {
    fn encode(self, encoder: &mut EncodedData) {
        self.clone().encode(encoder);
    }
}

impl<T: TypeConstructor<P>, P> TypeConstructor<(P,)> for &T {
    fn create_type_instance() -> String {
        T::create_type_instance()
    }
}

// Stub implementations for FnMut callbacks passed to JS
// These are used in methods like Array.every(), Array.forEach(), etc.
// Real wasm-bindgen handles these with wasm trampolines, but we provide stubs.
// Note: We only implement for `FnMut(...) -> R` since `FnMut(...)` is `FnMut(...) -> ()`.
macro_rules! impl_fnmut_stub {
    ($($arg:ident),*) => {
        impl<R, $($arg,)*> BinaryEncode for &mut dyn FnMut($($arg),*) -> R {
            fn encode(self, _encoder: &mut EncodedData) {
                panic!("FnMut callbacks are not yet supported in wry-bindgen");
            }
        }

        impl<R, $($arg,)*> TypeConstructor for &mut dyn FnMut($($arg),*) -> R {
            fn create_type_instance() -> String {
                "new window.CallbackType()".to_string()
            }
        }
    };
}

impl_fnmut_stub!();
impl_fnmut_stub!(A1);
impl_fnmut_stub!(A1, A2);
impl_fnmut_stub!(A1, A2, A3);
impl_fnmut_stub!(A1, A2, A3, A4);
impl_fnmut_stub!(A1, A2, A3, A4, A5);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6, A7);

// Slice encoding implementations - used for TypedArray constructors
macro_rules! impl_slice_encode {
    ($ty:ty, $push:ident) => {
        impl BinaryEncode for &[$ty] {
            fn encode(self, encoder: &mut EncodedData) {
                encoder.push_u32(self.len() as u32);
                for &val in self {
                    encoder.$push(val as _);
                }
            }
        }

        impl BinaryEncode for &mut [$ty] {
            fn encode(self, encoder: &mut EncodedData) {
                encoder.push_u32(self.len() as u32);
                for &val in self.iter() {
                    encoder.$push(val as _);
                }
            }
        }

        impl TypeConstructor for [$ty] {
            fn create_type_instance() -> String {
                concat!("new window.", stringify!($ty), "ArrayType()").to_string()
            }
        }

        // Explicit impls for slice references since [T] is not Sized
        impl TypeConstructor for &[$ty] {
            fn create_type_instance() -> String {
                <[$ty] as TypeConstructor>::create_type_instance()
            }
        }

        impl TypeConstructor for &mut [$ty] {
            fn create_type_instance() -> String {
                <[$ty] as TypeConstructor>::create_type_instance()
            }
        }
    };
}

impl_slice_encode!(u8, push_u8);
impl_slice_encode!(i8, push_u8);
impl_slice_encode!(u16, push_u16);
impl_slice_encode!(i16, push_u16);
impl_slice_encode!(u32, push_u32);
impl_slice_encode!(i32, push_u32);
impl_slice_encode!(u64, push_u64);
impl_slice_encode!(i64, push_u64);

impl BinaryEncode for &[f32] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for &val in self {
            encoder.push_u32(val.to_bits());
        }
    }
}

impl BinaryEncode for &mut [f32] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for &val in self.iter() {
            encoder.push_u32(val.to_bits());
        }
    }
}

impl TypeConstructor for [f32] {
    fn create_type_instance() -> String {
        "new window.Float32ArrayType()".to_string()
    }
}

impl TypeConstructor for &[f32] {
    fn create_type_instance() -> String {
        <[f32] as TypeConstructor>::create_type_instance()
    }
}

impl TypeConstructor for &mut [f32] {
    fn create_type_instance() -> String {
        <[f32] as TypeConstructor>::create_type_instance()
    }
}

impl BinaryEncode for &[f64] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for &val in self {
            encoder.push_u64(val.to_bits());
        }
    }
}

impl BinaryEncode for &mut [f64] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for &val in self.iter() {
            encoder.push_u64(val.to_bits());
        }
    }
}

impl TypeConstructor for [f64] {
    fn create_type_instance() -> String {
        "new window.Float64ArrayType()".to_string()
    }
}

impl TypeConstructor for &[f64] {
    fn create_type_instance() -> String {
        <[f64] as TypeConstructor>::create_type_instance()
    }
}

impl TypeConstructor for &mut [f64] {
    fn create_type_instance() -> String {
        <[f64] as TypeConstructor>::create_type_instance()
    }
}

/// Marker type for Rust callback parameter types.
pub struct RustCallbackMarker<P> {
    phantom: PhantomData<P>,
}
