//! JsValue, Closure, and reference encoding implementations.

use core::marker::PhantomData;

use wry_bindgen_core::Runtime;

use crate::Closure;
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use crate::value::JsValue;

use super::{BatchableResult, BinaryDecode, BinaryEncode, EncodeTypeDef, JsRef};

impl EncodeTypeDef for JsValue {
    fn encode_type_def(encoder: &mut crate::__rt::TypeDef) {
        <JsRef as EncodeTypeDef>::encode_type_def(encoder);
    }
}

impl BinaryEncode for JsValue {
    fn encode(self, encoder: &mut EncodedData) {
        self.js_ref().encode(encoder);
    }
}

impl BinaryDecode for JsValue {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        <JsRef as BinaryDecode>::decode(decoder).map(JsValue::from_ref)
    }
}

impl BatchableResult for JsValue {
    fn try_placeholder(batch: &mut Runtime) -> Option<Self> {
        <JsRef as BatchableResult>::try_placeholder(batch).map(JsValue::from_ref)
    }
}

impl<F: ?Sized> BatchableResult for Closure<F> {
    fn try_placeholder(batch: &mut Runtime) -> Option<Self> {
        Some(Closure {
            _phantom: PhantomData,
            callback: crate::closure::CallbackOwnership::None,
            value: JsValue::try_placeholder(batch)?,
        })
    }
}
