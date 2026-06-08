use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{DecodeError, DecodedData, EncodedData, JsRef};

pub trait BinaryEncode {
    fn encode(self, encoder: &mut EncodedData);
}

pub trait BinaryDecode: Sized {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError>;
}

pub trait JsRefEncode {
    fn js_ref(&self) -> JsRef;
}

/// Decode a value that a callback borrows for the duration of the call (the
/// wry-bindgen equivalent of wasm-bindgen's `RefFromWasmAbi`). The `Anchor`
/// owns whatever keeps the `&Self` valid across the closure invocation. The
/// trait lives here so the runtime's closure-encode impls can borrow-decode a
/// first argument; the impls themselves (JS handles, exported structs) live in
/// the higher-level crates that define those types.
pub trait RefFromBinaryDecode {
    /// The wire type JS sees for this borrowed reference.
    type Wire: EncodeTypeDef;

    /// The anchor type that keeps the decoded reference valid.
    type Anchor: core::ops::Deref<Target = Self>;

    /// Decode a reference anchor from binary data.
    fn ref_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError>;
}

impl RefFromBinaryDecode for str {
    type Wire = String;
    type Anchor = String;

    fn ref_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        String::decode(decoder)
    }
}

impl<T> RefFromBinaryDecode for [T]
where
    T: BinaryDecode + EncodeTypeDef,
{
    type Wire = Vec<T>;
    type Anchor = Vec<T>;

    fn ref_decode(decoder: &mut DecodedData) -> Result<Self::Anchor, DecodeError> {
        Vec::<T>::decode(decoder)
    }
}

pub(crate) const TYPE_CACHED: u8 = 0xFF;
pub(crate) const TYPE_FULL: u8 = 0xFE;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeTag {
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
    Callback = 18,
    Option = 19,
    Result = 20,
    Array = 21,
    BorrowedRef = 22,
    U8Clamped = 23,
    StringEnum = 24,
    DynamicUnion = 25,
    Char = 26,
    ThrowingResult = 27,
    NumericEnum = 28,
    RustValue = 29,
    RustBorrow = 30,
    // A `&mut [T]` argument. It rides the wire exactly like `Array` (a
    // length-prefixed element list), but the distinct tag tells the receiving
    // side to copy the (possibly mutated) elements back to the caller after the
    // call returns — wry has no shared linear memory, so the write-back travels
    // as data appended to the response. The element type def follows the tag.
    MutArray = 31,
}

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeDef {
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct FunctionTypeInfo<'a> {
    type_id: u32,
    can_use_cached: bool,
    type_def: &'a TypeDef,
}

impl<'a> FunctionTypeInfo<'a> {
    pub const fn new(type_id: u32, can_use_cached: bool, type_def: &'a TypeDef) -> Self {
        Self {
            type_id,
            can_use_cached,
            type_def,
        }
    }
}

impl BinaryEncode for FunctionTypeInfo<'_> {
    fn encode(self, encoder: &mut EncodedData) {
        if self.can_use_cached {
            encoder.push_u8(TYPE_CACHED);
            encoder.push_u32(self.type_id);
        } else {
            encoder.push_u8(TYPE_FULL);
            encoder.push_u32(self.type_id);
            for &byte in self.type_def.bytes() {
                encoder.push_u8(byte);
            }
        }
    }
}

impl TypeDef {
    pub fn of<T: EncodeTypeDef + ?Sized>() -> Self {
        let mut type_def = TypeDef::default();
        T::encode_type_def(&mut type_def);
        type_def
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn heap_ref(&mut self) {
        self.push_tag(TypeTag::HeapRef);
    }

    /// The wire type for an exported Rust struct passed or returned by value.
    /// On the wire it behaves exactly like a [`heap_ref`](Self::heap_ref) (the
    /// object wrapper rides the JS heap), but the distinct tag lets JS apply
    /// wasm-bindgen's moved-value semantics: passing the wrapper by value
    /// transfers ownership to Rust, so JS zeroes the wrapper's handle and a later
    /// use throws "Attempt to use a moved value".
    #[doc(hidden)]
    pub fn rust_value(&mut self, class_name: &str) {
        self.push_tag(TypeTag::RustValue);
        // The class name lets JS find an inheritance descendant's per-class
        // ancestor slot, so a by-value pass of a descendant as its ancestor can
        // be rejected (the descendant's own handle differs from the ancestor
        // view's). A non-participating struct has no such slot, so the check is a
        // no-op for it.
        self.push_str(class_name);
    }

    #[doc(hidden)]
    pub fn borrowed_ref(&mut self) {
        self.push_tag(TypeTag::BorrowedRef);
    }

    /// A borrowed `&T` to an exported struct: the routed object handle rides the
    /// wire as a plain `u32` (like a method receiver), so no borrow-stack
    /// round-trip is needed to read it. `class_name` lets JS route an inheritance
    /// descendant passed as `&Ancestor` to its shared ancestor-view handle.
    #[doc(hidden)]
    pub fn rust_borrow(&mut self, class_name: &str) {
        self.push_tag(TypeTag::RustBorrow);
        self.push_str(class_name);
    }

    /// Build the type def of `T` and, if it is an exported struct (`RustValue`
    /// tag), return its class name. Used to forward an exported struct's class
    /// name onto a borrowed `&T` argument so JS can route inheritance descendants
    /// to their ancestor view.
    #[doc(hidden)]
    pub fn rust_value_class_name<T: EncodeTypeDef + ?Sized>() -> Option<alloc::string::String> {
        let def = TypeDef::of::<T>();
        let bytes = def.bytes();
        if bytes.first().copied() != Some(TypeTag::RustValue as u8) {
            return None;
        }
        let len_bytes = bytes.get(1..5)?;
        let len = u32::from_le_bytes(len_bytes.try_into().ok()?) as usize;
        let name_bytes = bytes.get(5..5 + len)?;
        core::str::from_utf8(name_bytes).ok().map(Into::into)
    }

    #[doc(hidden)]
    pub fn u8_clamped(&mut self) {
        self.push_tag(TypeTag::U8Clamped);
    }

    /// A mutable array (`&mut [T]`): the inner array type def follows (an
    /// `Array` of `T`). JS copies the mutated elements back to the caller's
    /// array after the call.
    pub fn mut_array<T: EncodeTypeDef + ?Sized>(&mut self) {
        self.push_tag(TypeTag::MutArray);
        self.push_tag(TypeTag::Array);
        T::encode_type_def(self);
    }

    /// A mutable clamped array (`Clamped<&mut [u8]>`). The inner array type is a
    /// `U8Clamped` (the element is always `u8`); JS copies the mutated bytes back.
    pub fn mut_u8_clamped(&mut self) {
        self.push_tag(TypeTag::MutArray);
        self.push_tag(TypeTag::U8Clamped);
    }

    pub fn string_enum(&mut self, variants: &[&str]) {
        self.push_tag(TypeTag::StringEnum);
        self.push_u8(u8::try_from(variants.len()).expect("too many string enum variants"));
        for variant in variants {
            self.push_str(variant);
        }
    }

    /// A C-style enum: JS validates that a value is one of `values` (decoded as
    /// `i32` when `signed`, else `u32`) so a non-enum value is rejected rather
    /// than silently coerced.
    pub fn numeric_enum(&mut self, signed: bool, values: &[u32]) {
        self.push_tag(TypeTag::NumericEnum);
        self.push_u8(signed as u8);
        self.push_u8(u8::try_from(values.len()).expect("too many numeric enum variants"));
        for value in values {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    #[doc(hidden)]
    pub fn dynamic_union(
        &mut self,
        variant_count: usize,
        encode_variants: impl FnOnce(&mut TypeDef),
    ) {
        self.push_tag(TypeTag::DynamicUnion);
        self.push_u8(u8::try_from(variant_count).expect("too many dynamic union variants"));
        encode_variants(self);
    }

    #[doc(hidden)]
    pub fn dynamic_union_string_variant(&mut self, value: &str) {
        self.push_u8(0);
        self.push_str(value);
    }

    #[doc(hidden)]
    pub fn dynamic_union_type_variant<T: EncodeTypeDef>(&mut self) {
        self.push_u8(1);
        T::encode_type_def(self);
    }

    #[doc(hidden)]
    pub fn callback<Signature: EncodeTypeDef + ?Sized>(&mut self) {
        self.push_tag(TypeTag::Callback);
        Signature::encode_type_def(self);
    }

    #[doc(hidden)]
    pub fn callback_with_signature(
        &mut self,
        arg_count: u8,
        encode_args_and_return: impl FnOnce(&mut TypeDef),
    ) {
        self.push_tag(TypeTag::Callback);
        self.push_u8(arg_count);
        encode_args_and_return(self);
    }

    fn push_tag(&mut self, tag: TypeTag) {
        self.push_u8(tag as u8);
    }

    fn push_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn push_str(&mut self, value: &str) {
        self.bytes
            .extend_from_slice(&(value.len() as u32).to_le_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
    }
}

pub trait EncodeTypeDef {
    fn encode_type_def(type_def: &mut TypeDef);
}

impl EncodeTypeDef for () {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Null);
    }
}

impl BinaryEncode for () {
    fn encode(self, _encoder: &mut EncodedData) {}
}

impl BinaryDecode for () {
    fn decode(_decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(())
    }
}

macro_rules! impl_num {
    ($ty:ty, $tag:ident, $push:ident, $take:ident) => {
        impl EncodeTypeDef for $ty {
            fn encode_type_def(type_def: &mut TypeDef) {
                type_def.push_tag(TypeTag::$tag);
            }
        }

        impl BinaryEncode for $ty {
            fn encode(self, encoder: &mut EncodedData) {
                encoder.$push(self as _);
            }
        }

        impl BinaryDecode for $ty {
            fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
                Ok(decoder.$take()? as $ty)
            }
        }
    };
}

impl EncodeTypeDef for bool {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Bool);
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

impl EncodeTypeDef for char {
    fn encode_type_def(type_def: &mut TypeDef) {
        // The wire payload is the u32 code point, but JS converts to/from a 1-char
        // string (matching wasm-bindgen) via the `Char` type tag.
        type_def.push_tag(TypeTag::Char);
    }
}

impl BinaryEncode for char {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self as u32);
    }
}

impl BinaryDecode for char {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let cp = decoder.take_u32()?;
        char::from_u32(cp).ok_or_else(|| {
            DecodeError::custom(alloc::format!(
                "expected a valid Unicode scalar value, found {cp}"
            ))
        })
    }
}

impl_num!(u8, U8, push_u8, take_u8);
impl_num!(u16, U16, push_u16, take_u16);
impl_num!(u32, U32, push_u32, take_u32);
impl_num!(u64, U64, push_u64, take_u64);
impl_num!(u128, U128, push_u128, take_u128);
impl_num!(i8, I8, push_u8, take_u8);
impl_num!(i16, I16, push_u16, take_u16);
impl_num!(i32, I32, push_u32, take_u32);
impl_num!(i64, I64, push_u64, take_u64);
impl_num!(i128, I128, push_u128, take_u128);

impl EncodeTypeDef for f32 {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::F32);
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
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::F64);
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

impl EncodeTypeDef for usize {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Usize);
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

impl EncodeTypeDef for isize {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Isize);
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

impl EncodeTypeDef for str {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::String);
    }
}

impl EncodeTypeDef for &str {
    fn encode_type_def(type_def: &mut TypeDef) {
        str::encode_type_def(type_def);
    }
}

impl<T: JsRefEncode + ?Sized> EncodeTypeDef for &T {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.heap_ref();
    }
}

impl BinaryEncode for &str {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(self);
    }
}

impl<T: JsRefEncode + ?Sized> BinaryEncode for &T {
    fn encode(self, encoder: &mut EncodedData) {
        self.js_ref().raw().encode(encoder);
    }
}

impl EncodeTypeDef for String {
    fn encode_type_def(type_def: &mut TypeDef) {
        str::encode_type_def(type_def);
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
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Option);
        T::encode_type_def(type_def);
    }
}

impl<T: BinaryDecode> BinaryDecode for Option<T> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        if decoder.take_u8()? != 0 {
            Ok(Some(T::decode(decoder)?))
        } else {
            Ok(None)
        }
    }
}

impl<T: BinaryEncode> BinaryEncode for Option<T> {
    fn encode(self, encoder: &mut EncodedData) {
        match self {
            Some(value) => {
                encoder.push_u8(1);
                value.encode(encoder);
            }
            None => encoder.push_u8(0),
        }
    }
}

impl<T: EncodeTypeDef, E: EncodeTypeDef> EncodeTypeDef for Result<T, E> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Result);
        T::encode_type_def(type_def);
        E::encode_type_def(type_def);
    }
}

impl<T: BinaryEncode, E: BinaryEncode> BinaryEncode for Result<T, E> {
    fn encode(self, encoder: &mut EncodedData) {
        match self {
            Ok(value) => {
                encoder.push_u8(1);
                value.encode(encoder);
            }
            Err(error) => {
                encoder.push_u8(0);
                error.encode(encoder);
            }
        }
    }
}

impl<T: BinaryDecode, E: BinaryDecode> BinaryDecode for Result<T, E> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        if decoder.take_u8()? != 0 {
            Ok(Ok(T::decode(decoder)?))
        } else {
            Ok(Err(E::decode(decoder)?))
        }
    }
}

/// A `Result` returned from an exported Rust function. The wire payload is
/// identical to `Result`, but the distinct type tag tells JS to throw the `Err`
/// value as an exception instead of returning a `{err}` object, matching
/// wasm-bindgen's behavior for fallible exports.
pub struct ThrowingResult<T, E>(pub Result<T, E>);

impl<T: EncodeTypeDef, E: EncodeTypeDef> EncodeTypeDef for ThrowingResult<T, E> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::ThrowingResult);
        T::encode_type_def(type_def);
        E::encode_type_def(type_def);
    }
}

impl<T: BinaryEncode, E: BinaryEncode> BinaryEncode for ThrowingResult<T, E> {
    fn encode(self, encoder: &mut EncodedData) {
        self.0.encode(encoder);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for Vec<T> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Array);
        T::encode_type_def(type_def);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for &[T] {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Array);
        T::encode_type_def(type_def);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for &mut [T] {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.mut_array::<T>();
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for Box<[T]> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Array);
        T::encode_type_def(type_def);
    }
}

/// The wire form of a `&mut [T]` export argument. JS sends the array (under the
/// `MutArray` tag), Rust decodes it into an owned buffer the export mutates, and
/// the mutated buffer is copied back to JS in the response (`write_back`). The
/// export codegen advertises this type, decodes it, mutably borrows `buffer`,
/// then re-encodes it after the return value.
pub struct MutSliceArg<T> {
    pub buffer: Vec<T>,
}

impl<T: EncodeTypeDef> EncodeTypeDef for MutSliceArg<T> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.mut_array::<T>();
    }
}

impl<T: BinaryDecode> BinaryDecode for MutSliceArg<T> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(MutSliceArg {
            buffer: Vec::<T>::decode(decoder)?,
        })
    }
}

impl<T> MutSliceArg<T> {
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.buffer.as_mut_slice()
    }

    /// Append the (possibly mutated) elements to the export response so JS can
    /// copy them back into the caller's array.
    pub fn write_back(self, encoder: &mut EncodedData)
    where
        T: BinaryEncode,
    {
        encoder.push_u32(self.buffer.len() as u32);
        for val in self.buffer {
            val.encode(encoder);
        }
    }
}

impl<T> core::ops::Deref for MutSliceArg<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.buffer.as_slice()
    }
}

impl<T> core::ops::DerefMut for MutSliceArg<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.buffer.as_mut_slice()
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
            // A failed element decode means JS supplied an array element of the
            // wrong type; report it with wasm-bindgen's message.
            let item = T::decode(decoder)
                .map_err(|_| DecodeError::custom("array contains a value of the wrong type"))?;
            vec.push(item);
        }
        Ok(vec)
    }
}

impl<T: BinaryDecode> BinaryDecode for Box<[T]> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(Vec::<T>::decode(decoder)?.into_boxed_slice())
    }
}

macro_rules! ref_encode_via_clone {
    ($($ty:ty),* $(,)?) => {
        $(
            impl EncodeTypeDef for &$ty {
                fn encode_type_def(type_def: &mut TypeDef) {
                    <$ty as EncodeTypeDef>::encode_type_def(type_def);
                }
            }

            impl BinaryEncode for &$ty {
                fn encode(self, encoder: &mut EncodedData) {
                    self.clone().encode(encoder);
                }
            }
        )*
    };
}

ref_encode_via_clone!(
    bool, char, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize, String,
);

/// Register the write-back for a `&mut [T]` argument passed to a JS import.
///
/// wry has no shared linear memory, so a JS function that mutates a `&mut [T]`
/// argument cannot write into the Rust slice directly. Instead the receiver (JS)
/// appends the mutated array after the call's return value, and this queues a
/// closure that — once the return value is decoded — reads the array back and
/// copies it into the original slice. The whole encode -> call -> decode ->
/// write-back runs synchronously inside `run_js_sync`, so the raw slice pointer
/// captured here stays valid until the write-back runs.
fn register_slice_write_back<T: BinaryDecode + 'static>(slice: &mut [T]) {
    let ptr = slice.as_mut_ptr();
    let len = slice.len();
    crate::batch::push_write_back(Box::new(move |decoder: &mut DecodedData| {
        let updated = Vec::<T>::decode(decoder).expect("failed to decode &mut [T] write-back");
        // SAFETY: `ptr`/`len` describe the caller's `&mut [T]`, which outlives
        // this synchronous write-back (see the doc comment). JS returns an array
        // of the same length, but clamp to `len` defensively.
        let target = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        for (dst, src) in target.iter_mut().zip(updated) {
            *dst = src;
        }
    }));
}

macro_rules! slice_encode_via_copy {
    ($($ty:ty),* $(,)?) => {
        $(
            impl BinaryEncode for &[$ty] {
                fn encode(self, encoder: &mut EncodedData) {
                    encoder.push_u32(self.len() as u32);
                    for val in self {
                        (*val).encode(encoder);
                    }
                }
            }

            impl BinaryEncode for &mut [$ty] {
                fn encode(self, encoder: &mut EncodedData) {
                    encoder.push_u32(self.len() as u32);
                    for val in self.iter() {
                        (*val).encode(encoder);
                    }
                    register_slice_write_back(self);
                }
            }
        )*
    };
}

slice_encode_via_copy!(
    bool, char, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize
);

// A `&[String]` argument (e.g. a `slice_to_array` import taking `&[String]`).
// `String` is neither a copy-primitive nor a handle type, so it falls outside
// both the primitive and `JsRefEncode` slice impls; each element is cloned and
// encoded as a length-prefixed UTF-8 string, matching `Vec<String>`.
impl BinaryEncode for &[String] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.clone().encode(encoder);
        }
    }
}

impl BinaryEncode for &mut [String] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self.iter() {
            val.clone().encode(encoder);
        }
        register_slice_write_back(self);
    }
}

impl<T: JsRefEncode> BinaryEncode for &[T] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.js_ref().raw().encode(encoder);
        }
    }
}

impl<T: JsRefEncode + BinaryDecode + 'static> BinaryEncode for &mut [T] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self.iter() {
            val.js_ref().raw().encode(encoder);
        }
        register_slice_write_back(self);
    }
}

macro_rules! impl_fn_type_def {
    (0,) => {
        impl<R: EncodeTypeDef> EncodeTypeDef for fn() -> R {
            fn encode_type_def(type_def: &mut TypeDef) {
                type_def.push_u8(0);
                R::encode_type_def(type_def);
            }
        }
    };
    ($n:expr, $($T:ident),+) => {
        impl<$($T: EncodeTypeDef,)+ R: EncodeTypeDef> EncodeTypeDef for fn($($T),+) -> R {
            fn encode_type_def(type_def: &mut TypeDef) {
                type_def.push_u8($n);
                $($T::encode_type_def(type_def);)+
                R::encode_type_def(type_def);
            }
        }
    };
}

impl_fn_type_def!(0,);
impl_fn_type_def!(1, T1);
impl_fn_type_def!(2, T1, T2);
impl_fn_type_def!(3, T1, T2, T3);
impl_fn_type_def!(4, T1, T2, T3, T4);
impl_fn_type_def!(5, T1, T2, T3, T4, T5);
impl_fn_type_def!(6, T1, T2, T3, T4, T5, T6);
impl_fn_type_def!(7, T1, T2, T3, T4, T5, T6, T7);
impl_fn_type_def!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_fn_type_def!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_fn_type_def!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_fn_type_def!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_fn_type_def!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_fn_type_def!(13, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_fn_type_def!(
    14, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14
);
impl_fn_type_def!(
    15, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15
);
impl_fn_type_def!(
    16, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16
);
impl_fn_type_def!(
    17, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17
);
impl_fn_type_def!(
    18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18
);
impl_fn_type_def!(
    19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19
);
impl_fn_type_def!(
    20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20
);
impl_fn_type_def!(
    21, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21
);
impl_fn_type_def!(
    22, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22
);
impl_fn_type_def!(
    23, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23
);
impl_fn_type_def!(
    24, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24
);
impl_fn_type_def!(
    25, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25
);
impl_fn_type_def!(
    26, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26
);
impl_fn_type_def!(
    27, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27
);
impl_fn_type_def!(
    28, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28
);
impl_fn_type_def!(
    29, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28, T29
);
impl_fn_type_def!(
    30, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28, T29, T30
);
impl_fn_type_def!(
    31, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31
);
impl_fn_type_def!(
    32, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20,
    T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32
);
