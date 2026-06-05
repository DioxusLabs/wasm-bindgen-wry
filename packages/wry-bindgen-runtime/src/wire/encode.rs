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

    #[doc(hidden)]
    pub fn borrowed_ref(&mut self) {
        self.push_tag(TypeTag::BorrowedRef);
    }

    #[doc(hidden)]
    pub fn u8_clamped(&mut self) {
        self.push_tag(TypeTag::U8Clamped);
    }

    pub fn string_enum(&mut self, variants: &[&str]) {
        self.push_tag(TypeTag::StringEnum);
        self.push_u8(u8::try_from(variants.len()).expect("too many string enum variants"));
        for variant in variants {
            self.push_str(variant);
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
        type_def.push_tag(TypeTag::U32);
    }
}

impl BinaryEncode for char {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self as u32);
    }
}

impl BinaryDecode for char {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        char::from_u32(decoder.take_u32()?)
            .ok_or_else(|| DecodeError::custom("invalid char scalar value"))
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
        type_def.push_tag(TypeTag::Array);
        T::encode_type_def(type_def);
    }
}

impl<T: EncodeTypeDef> EncodeTypeDef for Box<[T]> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.push_tag(TypeTag::Array);
        T::encode_type_def(type_def);
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
                    for val in self {
                        (*val).encode(encoder);
                    }
                }
            }
        )*
    };
}

slice_encode_via_copy!(
    bool, char, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize
);

impl<T: JsRefEncode> BinaryEncode for &[T] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.js_ref().raw().encode(encoder);
        }
    }
}

impl<T: JsRefEncode> BinaryEncode for &mut [T] {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self.len() as u32);
        for val in self {
            val.js_ref().raw().encode(encoder);
        }
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
