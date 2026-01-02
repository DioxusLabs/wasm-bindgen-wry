//! Core encoding and decoding traits for the binary protocol.
//!
//! This module provides traits for serializing and deserializing Rust types
//! to/from the binary IPC protocol.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::marker::PhantomData;
use slotmap::Key;

use crate::Closure;
use crate::WasmClosureFnOnce;
use crate::batch::{BATCH_STATE, BatchState};
use crate::convert::RefFromBinaryDecode;
use crate::function::{RustCallback, register_value};
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use crate::value::JsValue;

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

/// Trait for converting a closure into a Closure wrapper.
/// This trait is used instead of `From` to allow blanket implementations
/// for all closure types without conflicting with other `From` impls.
pub trait IntoClosure<M> {
    type Output;
    fn into_closure(self) -> Self::Output;
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

/// Marker for cached type definition (type already sent, just reference by ID)
/// Format: [TYPE_CACHED] [type_id: u32]
pub const TYPE_CACHED: u8 = 0xFF;

/// Marker for full type definition (first time sending this type signature)
/// Format: [TYPE_FULL] [type_id: u32] [param_count: u8] [param TypeDefs...] [return TypeDef]
pub const TYPE_FULL: u8 = 0xFE;

/// Type tags for the binary type definition protocol.
/// Used to encode type information that JavaScript can parse to create TypeClass instances.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTag {
    // Primitive types
    Null = 0,
    Bool = 1,
    U8 = 2,
    U16 = 3,
    U32 = 4,
    U64 = 5,
    U128 = 6,
    I8 = 7,
    I16 = 8,
    I32 = 9,
    I64 = 10,
    I128 = 11,
    F32 = 12,
    F64 = 13,
    Usize = 14,
    Isize = 15,
    String = 16,
    HeapRef = 17,
    // Compound types
    /// Callback type: followed by param_count (u8), param TypeDefs..., return TypeDef
    Callback = 18,
    /// Option type: followed by inner TypeDef. Encodes as u8 flag (0=None, 1=Some) + value if Some
    Option = 19,
    /// Result type: followed by ok TypeDef and err TypeDef. Encodes as u8 flag (0=Err, 1=Ok) + value
    Result = 20,
    /// Array type: followed by element TypeDef. Encodes as u32 length + elements
    Array = 21,
    /// Borrowed reference: uses the borrow stack (indices 1-127) instead of the heap.
    /// Automatically cleaned up after each operation completes.
    BorrowedRef = 22,
    /// Clamped u8 array type: represents Uint8ClampedArray in JS.
    /// Element type is always u8. Encodes as u32 length + u8 elements.
    U8Clamped = 23,
}

/// Trait for types that can encode their type definition into the binary protocol.
/// This is used to send type information to JavaScript for callback arguments.
pub trait EncodeTypeDef {
    /// Encode this type's definition into the buffer.
    /// For primitives, this is just the TypeTag byte.
    /// For callbacks, this includes param count, param types, and return type.
    fn encode_type_def(buf: &mut Vec<u8>);
}

// Unit type implementations

impl BatchableResult for () {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(_: &mut BatchState) -> Self {}
}

impl EncodeTypeDef for () {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::Null as u8);
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

impl EncodeTypeDef for bool {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::Bool as u8);
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

impl EncodeTypeDef for u8 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U8 as u8);
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

impl EncodeTypeDef for u16 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U16 as u8);
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

impl EncodeTypeDef for u32 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U32 as u8);
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

impl EncodeTypeDef for u64 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U64 as u8);
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

impl EncodeTypeDef for u128 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U128 as u8);
    }
}

impl BinaryEncode for u128 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u128(self);
    }
}

impl BinaryDecode for u128 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u128()?)
    }
}

impl EncodeTypeDef for i8 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::I8 as u8);
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

impl EncodeTypeDef for i16 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::I16 as u8);
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

impl EncodeTypeDef for i32 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::I32 as u8);
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

impl EncodeTypeDef for i64 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::I64 as u8);
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

impl EncodeTypeDef for i128 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::I128 as u8);
    }
}

impl BinaryEncode for i128 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u128(self as u128);
    }
}

impl BinaryDecode for i128 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(decoder.take_u128()? as i128)
    }
}

impl EncodeTypeDef for f32 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::F32 as u8);
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

impl EncodeTypeDef for f64 {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::F64 as u8);
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

impl EncodeTypeDef for usize {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::Usize as u8);
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

impl EncodeTypeDef for isize {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::Isize as u8);
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

impl EncodeTypeDef for str {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::String as u8);
    }
}

// Explicit impl for &str since str is not Sized and blanket impl doesn't apply
impl EncodeTypeDef for &str {
    fn encode_type_def(buf: &mut Vec<u8>) {
        <str as EncodeTypeDef>::encode_type_def(buf);
    }
}

// Blanket impl for &T references
impl<T: EncodeTypeDef> EncodeTypeDef for &T {
    fn encode_type_def(buf: &mut Vec<u8>) {
        T::encode_type_def(buf);
    }
}

impl BinaryEncode for &str {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(self);
    }
}

impl EncodeTypeDef for String {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::String as u8);
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

impl<T: EncodeTypeDef> EncodeTypeDef for Option<T> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Option encodes as: [Option tag] [inner type]
        // Actual values encode as: [u8 flag (0=None, 1=Some)] [value if Some]
        buf.push(TypeTag::Option as u8);
        T::encode_type_def(buf);
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

impl<T: EncodeTypeDef, E: EncodeTypeDef> EncodeTypeDef for Result<T, E> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Result encodes as: [Result tag] [ok type] [err type]
        buf.push(TypeTag::Result as u8);
        T::encode_type_def(buf);
        E::encode_type_def(buf);
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

impl EncodeTypeDef for JsValue {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::HeapRef as u8);
    }
}

impl BinaryEncode for JsValue {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.id());
    }
}

impl BinaryDecode for JsValue {
    fn decode(_: &mut DecodedData) -> Result<Self, DecodeError> {
        // JS value is always in sync with the dom. We should never need to decode it.
        BATCH_STATE.with(|state| {
            let mut batch = state.borrow_mut();
            Ok(Self::batched_placeholder(&mut batch))
        })
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

impl<F: ?Sized> BatchableResult for Closure<F> {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(batch: &mut BatchState) -> Self {
        Closure {
            _phantom: PhantomData,
            value: JsValue::batched_placeholder(batch),
        }
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
    bool, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, isize, usize, f32, f64, String
);

/// Marker trait for types that can be cheaply cloned for encoding.
pub trait CloneForEncode: Clone {}

impl CloneForEncode for bool {}
impl CloneForEncode for u8 {}
impl CloneForEncode for u16 {}
impl CloneForEncode for u32 {}
impl CloneForEncode for u64 {}
impl CloneForEncode for i8 {}
impl CloneForEncode for i16 {}
impl CloneForEncode for i32 {}
impl CloneForEncode for i64 {}
impl CloneForEncode for f32 {}
impl CloneForEncode for f64 {}
impl CloneForEncode for usize {}
impl CloneForEncode for isize {}
impl CloneForEncode for String {}

// Blanket implementation for references to types that implement CloneForEncode
// Note: We only implement for P=() to avoid conflicts with RustCallbackMarker impls
impl<T: BinaryEncode + CloneForEncode> BinaryEncode for &T {
    fn encode(self, encoder: &mut EncodedData) {
        self.clone().encode(encoder);
    }
}

// When encoding JsValue references, encode the underlying ID
impl BinaryEncode for &JsValue {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.id());
    }
}

/// Wrapper type that encodes a callback registration key with Callback type info.
/// This tells JS to create a RustFunction wrapper when decoding the value.
/// The type parameter F should be `dyn FnMut(...) -> R` to capture the callback signature.
pub struct CallbackKey<F: ?Sized>(pub u64, pub PhantomData<F>);

impl<F: ?Sized> BinaryEncode for CallbackKey<F> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.0);
    }
}

// Blanket impl: All Closures encode as HeapRef since they're JS heap references
impl<T: ?Sized> EncodeTypeDef for crate::Closure<T> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        JsValue::encode_type_def(buf);
    }
}

/// Helper macro to decode callback arguments and execute a body.
///
/// Usage: decode_args!(decoder; [type1, type2, ...] => body)
/// The body can use the type names as variables containing the decoded arguments.
macro_rules! decode_args {
    // Main entry: decode each arg and call body
    ($decoder:expr; [$first:ident, $($ty:ident,)*] => $body:expr) => {{
        #[allow(non_snake_case)]
        let $first = <$first as BinaryDecode>::decode($decoder).unwrap();
        decode_args!($decoder; [$($ty,)*] => $body);
    }};
    // Nothing to decode, just execute body
    ($decoder:expr; [] => $body:expr) => {{
        $body
    }};
}

macro_rules! impl_fnmut_stub {
    ($($arg:ident),*) => {
        // Implement EncodeTypeDef for fn(owned*) -> R
        impl<R, $($arg,)*> EncodeTypeDef for CallbackKey<fn($($arg),*) -> R>
            where
            $($arg: EncodeTypeDef + 'static, )*
            R: EncodeTypeDef + 'static,
        {
            #[allow(unused)]
            fn encode_type_def(buf: &mut Vec<u8>) {
                buf.push(TypeTag::Callback as u8);
                // Encode arg count
                let mut count: u8 = 0;
                $(
                    let _ = PhantomData::<$arg>;
                    count += 1;
                )*
                buf.push(count);
                // Encode each argument type
                $(<$arg as EncodeTypeDef>::encode_type_def(buf);)*
                // Encode return type
                <R as EncodeTypeDef>::encode_type_def(buf);
            }
        }

        // Implement WasmClosure trait for dyn FnMut variants
        impl<R, $($arg,)*> crate::WasmClosure<fn($($arg),*) -> R> for dyn FnMut($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(mut boxed: Box<Self>) -> crate::Closure<Self> {
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = boxed($($arg),*);
                            result.encode(encoder);
                        });
                    },
                ));
                // Use wbg_cast with CallbackKey so param encodes as Callback type (JS creates RustFunction)
                // Return type is Closure which encodes as HeapRef (JS inserts into heap)
                $crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::Closure<Self>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }

        // Implement WasmClosure trait for dyn Fn variants (immutable closures)
        // These CAN be called reentrantly since Fn only needs &self
        impl<R, $($arg,)*> crate::WasmClosure<fn($($arg),*) -> R> for dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(boxed: Box<Self>) -> crate::Closure<Self> {
                let key = register_value(RustCallback::new_fn(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = boxed($($arg),*);
                            result.encode(encoder);
                        });
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::Closure<Self>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }

        impl<R, F, $($arg,)*> IntoClosure<fn($($arg),*) -> R> for F
            where F: FnMut($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            type Output = crate::Closure<dyn FnMut($($arg),*) -> R>;

            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(mut self) -> Self::Output {
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = self($($arg),*);
                            result.encode(encoder);
                        });
                    },
                ));
                // Use wbg_cast with CallbackKey so param encodes as Callback type (JS creates RustFunction)
                // Return type is Closure which encodes as HeapRef (JS inserts into heap)
                $crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, Self::Output>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }

    };
}

/// Macro to implement EncodeTypeDef and BinaryEncode for closure reference types.
/// These are used by js-sys bindings like `&mut dyn FnMut(JsValue, u32, Array) -> bool`.
/// Unlike the WasmClosure impls above, these use simple BinaryDecode arguments without markers.
macro_rules! impl_closure_ref_encode {
    ($($arg:ident),*) => {
        // Implement EncodeTypeDef for &mut dyn FnMut(...) -> R
        impl<R, $($arg,)*> EncodeTypeDef for &mut dyn FnMut($($arg),*) -> R
            where
            $($arg: EncodeTypeDef + 'static, )*
            R: EncodeTypeDef + 'static,
        {
            #[allow(unused)]
            fn encode_type_def(buf: &mut Vec<u8>) {
                buf.push(TypeTag::Callback as u8);
                // Encode arg count
                let mut count: u8 = 0;
                $(
                    let _ = PhantomData::<$arg>;
                    count += 1;
                )*
                buf.push(count);
                // Encode each argument type
                $(<$arg as EncodeTypeDef>::encode_type_def(buf);)*
                // Encode return type
                <R as EncodeTypeDef>::encode_type_def(buf);
            }
        }

        // Implement BinaryEncode for &mut dyn FnMut(...) -> R
        impl<R, $($arg,)*> BinaryEncode for &mut dyn FnMut($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn encode(self, encoder: &mut EncodedData) {
                // Decompose fat pointer to (data_ptr, vtable_ptr) to erase the lifetime.
                // SAFETY: The closure reference must remain valid for the duration of the JS call.
                // This is safe because JS callbacks are invoked synchronously during the call.
                let ptr = self as *mut dyn FnMut($($arg),*) -> R;
                let (data_ptr, vtable_ptr): (usize, usize) = unsafe { core::mem::transmute(ptr) };

                // Register a temporary callback that calls through the pointer
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // SAFETY: The pointer is valid for the duration of the JS call.
                        // Reconstruct the fat pointer from the stored components.
                        let ptr: *mut dyn FnMut($($arg),*) -> R = unsafe {
                            core::mem::transmute((data_ptr, vtable_ptr))
                        };
                        let f: &mut dyn FnMut($($arg),*) -> R = unsafe { &mut *ptr };
                        // Decode arguments and call the closure
                        $(let $arg = <$arg as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                    },
                ));
                encoder.push_u64(key.data().as_ffi());
            }
        }

        // Implement EncodeTypeDef for &dyn Fn(...) -> R
        impl<R, $($arg,)*> EncodeTypeDef for &dyn Fn($($arg),*) -> R
            where
            $($arg: EncodeTypeDef + 'static, )*
            R: EncodeTypeDef + 'static,
        {
            #[allow(unused)]
            fn encode_type_def(buf: &mut Vec<u8>) {
                buf.push(TypeTag::Callback as u8);
                // Encode arg count
                let mut count: u8 = 0;
                $(
                    let _ = PhantomData::<$arg>;
                    count += 1;
                )*
                buf.push(count);
                // Encode each argument type
                $(<$arg as EncodeTypeDef>::encode_type_def(buf);)*
                // Encode return type
                <R as EncodeTypeDef>::encode_type_def(buf);
            }
        }

        // Implement BinaryEncode for &dyn Fn(...) -> R (supports reentrant calls)
        impl<R, $($arg,)*> BinaryEncode for &dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn encode(self, encoder: &mut EncodedData) {
                // Decompose fat pointer to (data_ptr, vtable_ptr) to erase the lifetime.
                // SAFETY: The closure reference must remain valid for the duration of the JS call.
                // This is safe because JS callbacks are invoked synchronously during the call.
                let ptr = self as *const dyn Fn($($arg),*) -> R;
                let (data_ptr, vtable_ptr): (usize, usize) = unsafe { core::mem::transmute(ptr) };

                // Register a temporary callback that calls through the pointer
                let key = register_value(RustCallback::new_fn(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // SAFETY: The pointer is valid for the duration of the JS call.
                        // Reconstruct the fat pointer from the stored components.
                        let ptr: *const dyn Fn($($arg),*) -> R = unsafe {
                            core::mem::transmute((data_ptr, vtable_ptr))
                        };
                        let f: &dyn Fn($($arg),*) -> R = unsafe { &*ptr };
                        // Decode arguments and call the closure
                        $(let $arg = <$arg as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                    },
                ));
                encoder.push_u64(key.data().as_ffi());
            }
        }

        // Implement EncodeTypeDef for &mut dyn Fn(...) -> R
        impl<R, $($arg,)*> EncodeTypeDef for &mut dyn Fn($($arg),*) -> R
            where
            $($arg: EncodeTypeDef + 'static, )*
            R: EncodeTypeDef + 'static,
        {
            #[allow(unused)]
            fn encode_type_def(buf: &mut Vec<u8>) {
                buf.push(TypeTag::Callback as u8);
                // Encode arg count
                let mut count: u8 = 0;
                $(
                    let _ = PhantomData::<$arg>;
                    count += 1;
                )*
                buf.push(count);
                // Encode each argument type
                $(<$arg as EncodeTypeDef>::encode_type_def(buf);)*
                // Encode return type
                <R as EncodeTypeDef>::encode_type_def(buf);
            }
        }

        // Implement BinaryEncode for &mut dyn Fn(...) -> R (supports reentrant calls)
        impl<R, $($arg,)*> BinaryEncode for &mut dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn encode(self, encoder: &mut EncodedData) {
                // Decompose fat pointer to (data_ptr, vtable_ptr) to erase the lifetime.
                // SAFETY: The closure reference must remain valid for the duration of the JS call.
                // This is safe because JS callbacks are invoked synchronously during the call.
                // We use *const because Fn only requires & to call
                let ptr = self as *const dyn Fn($($arg),*) -> R;
                let (data_ptr, vtable_ptr): (usize, usize) = unsafe { core::mem::transmute(ptr) };

                // Register a temporary callback that calls through the pointer
                let key = register_value(RustCallback::new_fn(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // SAFETY: The pointer is valid for the duration of the JS call.
                        // Reconstruct the fat pointer from the stored components.
                        let ptr: *const dyn Fn($($arg),*) -> R = unsafe {
                            core::mem::transmute((data_ptr, vtable_ptr))
                        };
                        let f: &dyn Fn($($arg),*) -> R = unsafe { &*ptr };
                        // Decode arguments and call the closure
                        $(let $arg = <$arg as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                    },
                ));
                encoder.push_u64(key.data().as_ffi());
            }
        }
    };
}

impl_closure_ref_encode!();
impl_closure_ref_encode!(A1);
impl_closure_ref_encode!(A1, A2);
impl_closure_ref_encode!(A1, A2, A3);
impl_closure_ref_encode!(A1, A2, A3, A4);
impl_closure_ref_encode!(A1, A2, A3, A4, A5);
impl_closure_ref_encode!(A1, A2, A3, A4, A5, A6);
impl_closure_ref_encode!(A1, A2, A3, A4, A5, A6, A7);

impl_fnmut_stub!();
impl_fnmut_stub!(A1);
impl_fnmut_stub!(A1, A2);
impl_fnmut_stub!(A1, A2, A3);
impl_fnmut_stub!(A1, A2, A3, A4);
impl_fnmut_stub!(A1, A2, A3, A4, A5);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6, A7);

/// Marker type for closures that borrow the first argument.
pub struct BorrowedFirstArg;

/// Macro to implement WasmClosure and IntoClosure for closures that borrow the first argument.
/// This uses RefFromBinaryDecode for the first arg and BinaryDecode for the rest.
macro_rules! impl_fnmut_stub_ref {
    ($first:ident $(, $rest:ident)*) => {
        // Implement EncodeTypeDef for fn(borrowed, owned*) -> R
        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> EncodeTypeDef for CallbackKey<fn(&$first, $($rest),*) -> R>
            where
            $first: EncodeTypeDef + 'static,
            $($rest: EncodeTypeDef + 'static, )*
            R: EncodeTypeDef + 'static,
        {
            #[allow(unused)]
            fn encode_type_def(buf: &mut Vec<u8>) {
                buf.push(TypeTag::Callback as u8);
                // Encode arg count
                let mut count: u8 = 1;
                $(
                    let _ = PhantomData::<$rest>;
                    count += 1;
                )*
                buf.push(count);
                // Encode each argument type
                buf.push(TypeTag::BorrowedRef as u8);
                $(<$rest as EncodeTypeDef>::encode_type_def(buf);)*
                // Encode return type
                <R as EncodeTypeDef>::encode_type_def(buf);
            }
        }

        // WasmClosure for dyn FnMut(&First, ...) -> R
        impl<R, $first, $($rest,)*> crate::WasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for dyn FnMut(&$first, $($rest),*) -> R
            where
            $first: RefFromBinaryDecode + EncodeTypeDef + 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(mut boxed: Box<Self>) -> crate::Closure<Self> {
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let anchor = <$first as RefFromBinaryDecode>::ref_decode(decoder).unwrap();
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = boxed(&*anchor, $($rest),*);
                        result.encode(encoder);
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn(&$first, $($rest),*) -> R>, crate::Closure<Self>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }

        // WasmClosure for dyn Fn(&First, ...) -> R (supports reentrant calls)
        impl<R, $first, $($rest,)*> crate::WasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for dyn Fn(&$first, $($rest),*) -> R
            where
            $first: RefFromBinaryDecode + EncodeTypeDef + 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(boxed: Box<Self>) -> crate::Closure<Self> {
                let key = register_value(RustCallback::new_fn(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let anchor = <$first as RefFromBinaryDecode>::ref_decode(decoder).unwrap();
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = boxed(&*anchor, $($rest),*);
                        result.encode(encoder);
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn(&$first, $($rest),*) -> R>, crate::Closure<Self>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }

        // IntoClosure for F: FnMut(&First, ...) -> R
        impl<R, F, $first, $($rest,)*> IntoClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for F
            where F: FnMut(&$first, $($rest),*) -> R + 'static,
            $first: RefFromBinaryDecode + EncodeTypeDef + 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            type Output = crate::Closure<dyn FnMut(&$first, $($rest),*) -> R>;

            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(mut self) -> Self::Output {
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let anchor = <$first as RefFromBinaryDecode>::ref_decode(decoder).unwrap();
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = self(&*anchor, $($rest),*);
                        result.encode(encoder);
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn(&$first, $($rest),*) -> R>, Self::Output>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }
    };
}

impl_fnmut_stub_ref!(A1);
impl_fnmut_stub_ref!(A1, A2);
impl_fnmut_stub_ref!(A1, A2, A3);
impl_fnmut_stub_ref!(A1, A2, A3, A4);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5, A6);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5, A6, A7);

/// Macro to implement WasmClosureFnOnce for FnOnce closures of various arities.
/// This wraps an FnOnce in an FnMut that panics if called more than once.
macro_rules! impl_fn_once {
    ($($arg:ident),*) => {
        impl<R, F, $($arg,)*> WasmClosureFnOnce<dyn FnMut($($arg),*) -> R, fn($($arg),*) -> R> for F
        where
            F: FnOnce($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut($($arg),*) -> R> {
                // Use Option to allow taking the FnOnce
                let mut me = Some(self);
                // Register the callback using the same pattern as impl_fnmut_stub
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let f = me.take().expect("FnOnce closure called more than once");
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = f($($arg),*);
                            result.encode(encoder);
                        });
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, Closure<dyn FnMut($($arg),*) -> R>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }
    };
}

impl_fn_once!();
impl_fn_once!(A1);
impl_fn_once!(A1, A2);
impl_fn_once!(A1, A2, A3);
impl_fn_once!(A1, A2, A3, A4);
impl_fn_once!(A1, A2, A3, A4, A5);
impl_fn_once!(A1, A2, A3, A4, A5, A6);
impl_fn_once!(A1, A2, A3, A4, A5, A6, A7);

/// Macro to implement WasmClosureFnOnce for FnOnce closures that borrow the first argument.
/// This uses RefFromBinaryDecode for the first arg and BinaryDecode for the rest.
macro_rules! impl_fn_once_ref {
    ($first:ident $(, $rest:ident)*) => {
        impl<R, F, $first, $($rest,)*> WasmClosureFnOnce<dyn FnMut(&$first, $($rest),*) -> R, (BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for F
        where
            F: FnOnce(&$first, $($rest),*) -> R + 'static,
            $first: RefFromBinaryDecode + EncodeTypeDef + 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                let mut me = Some(self);
                let key = register_value(RustCallback::new_fn_mut(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let f = me.take().expect("FnOnce closure called more than once");
                        let anchor = <$first as RefFromBinaryDecode>::ref_decode(decoder).unwrap();
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder).unwrap();)*
                        let result = f(&*anchor, $($rest),*);
                        result.encode(encoder);
                    },
                ));
                $crate::__rt::wbg_cast::<CallbackKey<fn(&$first, $($rest),*) -> R>, Closure<dyn FnMut(&$first, $($rest),*) -> R>>(
                    CallbackKey(key.data().as_ffi(), PhantomData)
                )
            }
        }
    };
}

impl_fn_once_ref!(A1);
impl_fn_once_ref!(A1, A2);
impl_fn_once_ref!(A1, A2, A3);
impl_fn_once_ref!(A1, A2, A3, A4);
impl_fn_once_ref!(A1, A2, A3, A4, A5);
impl_fn_once_ref!(A1, A2, A3, A4, A5, A6);
impl_fn_once_ref!(A1, A2, A3, A4, A5, A6, A7);

impl<F: ?Sized> BinaryDecode for crate::Closure<F> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        // Decode the JsValue wrapping the closure
        let value = <crate::JsValue as BinaryDecode>::decode(decoder)?;
        Ok(Self {
            _phantom: PhantomData,
            value,
        })
    }
}

impl<F: ?Sized> BinaryEncode for crate::Closure<F> {
    fn encode(self, encoder: &mut EncodedData) {
        // Encode the JsValue
        self.value.encode(encoder);
    }
}

impl<F: ?Sized> BinaryEncode for &crate::Closure<F> {
    fn encode(self, encoder: &mut EncodedData) {
        // Encode the JsValue
        (&self.value).encode(encoder);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for Vec<T> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Array type tag followed by element type
        buf.push(TypeTag::Array as u8);
        T::encode_type_def(buf);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for &[T] {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Array type tag followed by element type
        buf.push(TypeTag::Array as u8);
        T::encode_type_def(buf);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for &mut [T] {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Array type tag followed by element type
        buf.push(TypeTag::Array as u8);
        T::encode_type_def(buf);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for Box<[T]> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        // Array type tag followed by element type
        buf.push(TypeTag::Array as u8);
        T::encode_type_def(buf);
    }
}

impl<T: BinaryEncode> BinaryEncode for Box<[T]> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self.into_vec() {
            val.encode(encoder);
        }
    }
}

impl<T: BinaryEncode> BinaryEncode for Vec<T> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.encode(encoder);
        }
    }
}

impl<T: BinaryDecode> BinaryDecode for Vec<T> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let len = decoder.take_u32()? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode(decoder)?);
        }
        Ok(vec)
    }
}

impl<T: BinaryDecode> BatchableResult for Vec<T> {
    fn needs_flush() -> bool {
        true
    }

    fn batched_placeholder(_batch: &mut BatchState) -> Self {
        unreachable!("needs_flush types should never call batched_placeholder")
    }
}

impl<T> BinaryEncode for &[T]
where
    for<'a> &'a T: BinaryEncode,
{
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.encode(encoder);
        }
    }
}

impl<T> BinaryEncode for &mut [T]
where
    for<'a> &'a T: BinaryEncode,
{
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.encode(encoder);
        }
    }
}

// ============ Clamped<T> implementations ============

use crate::Clamped;

impl EncodeTypeDef for Clamped<Vec<u8>> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U8Clamped as u8);
    }
}

impl<'a> EncodeTypeDef for Clamped<&'a [u8]> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U8Clamped as u8);
    }
}

impl<'a> EncodeTypeDef for Clamped<&'a mut [u8]> {
    fn encode_type_def(buf: &mut Vec<u8>) {
        buf.push(TypeTag::U8Clamped as u8);
    }
}

impl BinaryEncode for Clamped<Vec<u8>> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.0.len() as u32);
        for val in self.0 {
            encoder.push_u8(val);
        }
    }
}

impl<'a> BinaryEncode for Clamped<&'a [u8]> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.0.len() as u32);
        for &val in self.0 {
            encoder.push_u8(val);
        }
    }
}

impl<'a> BinaryEncode for Clamped<&'a mut [u8]> {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.0.len() as u32);
        for &mut val in self.0 {
            encoder.push_u8(val);
        }
    }
}

impl BinaryDecode for Clamped<Vec<u8>> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let len = decoder.take_u32()? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(decoder.take_u8()?);
        }
        Ok(Clamped(vec))
    }
}

impl BatchableResult for Clamped<Vec<u8>> {
    fn needs_flush() -> bool {
        true
    }

    fn batched_placeholder(_batch: &mut BatchState) -> Self {
        unreachable!("needs_flush types should never call batched_placeholder")
    }
}
