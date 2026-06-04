use alloc::string::String;
use alloc::vec::Vec;

use wry_bindgen_abi::{BinaryDecode, BinaryEncode, EncodedData, JsRef, ObjectHandle};

use crate::Runtime;

pub trait BatchableResult: BinaryDecode {
    fn try_placeholder(_: &mut Runtime) -> Option<Self> {
        None
    }
}

#[derive(Clone, Copy, Default)]
pub struct RequireFlush;

impl BinaryEncode for RequireFlush {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.mark_needs_flush();
    }
}

impl BatchableResult for () {
    fn try_placeholder(_: &mut Runtime) -> Option<Self> {
        Some(())
    }
}

macro_rules! impl_value_type {
    ($($ty:ty),*) => {
        $(impl BatchableResult for $ty {})*
    };
}

impl_value_type!(
    bool, char, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, isize, usize, f32, f64, String
);

impl<T: BinaryDecode> BatchableResult for Option<T> {}

impl<T: BinaryDecode, E: BinaryDecode> BatchableResult for Result<T, E> {}

impl<T: BinaryDecode> BatchableResult for Vec<T> {}

impl BatchableResult for JsRef {
    fn try_placeholder(batch: &mut Runtime) -> Option<Self> {
        Some(batch.next_placeholder_ref())
    }
}

impl BatchableResult for ObjectHandle {}
