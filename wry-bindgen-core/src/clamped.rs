//! Support for JavaScript `Uint8ClampedArray` values.

use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

use wry_bindgen_abi::{
    BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, EncodedData, TypeDef,
};

use crate::BatchableResult;

/// A wrapper type around slices and vectors for binding `Uint8ClampedArray`.
#[derive(Copy, Clone, PartialEq, Debug, Eq)]
pub struct Clamped<T>(pub T);

impl<T> Deref for Clamped<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Clamped<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl EncodeTypeDef for Clamped<Vec<u8>> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.u8_clamped();
    }
}

impl EncodeTypeDef for Clamped<&[u8]> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.u8_clamped();
    }
}

impl EncodeTypeDef for Clamped<&mut [u8]> {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.u8_clamped();
    }
}

impl BinaryEncode for Clamped<Vec<u8>> {
    fn encode(self, encoder: &mut EncodedData) {
        (self.0.len() as u32).encode(encoder);
        for val in self.0 {
            val.encode(encoder);
        }
    }
}

impl BinaryEncode for Clamped<&[u8]> {
    fn encode(self, encoder: &mut EncodedData) {
        (self.0.len() as u32).encode(encoder);
        for &val in self.0 {
            val.encode(encoder);
        }
    }
}

impl BinaryEncode for Clamped<&mut [u8]> {
    fn encode(self, encoder: &mut EncodedData) {
        (self.0.len() as u32).encode(encoder);
        for &mut val in self.0 {
            val.encode(encoder);
        }
    }
}

impl BinaryDecode for Clamped<Vec<u8>> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        let len = u32::decode(decoder)? as usize;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(u8::decode(decoder)?);
        }
        Ok(Clamped(vec))
    }
}

impl BatchableResult for Clamped<Vec<u8>> {}
