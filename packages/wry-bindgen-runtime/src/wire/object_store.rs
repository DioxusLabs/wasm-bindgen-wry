use super::{BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodedData};

#[derive(Clone, Copy)]
pub struct ObjectHandle(u32);

impl ObjectHandle {
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) fn raw(self) -> u32 {
        self.0
    }

    /// Release this stored Rust object.
    pub fn drop_rust_object(self) {
        crate::batch::drop_rust_object(self);
    }
}

impl BinaryDecode for ObjectHandle {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(ObjectHandle::from_raw(u32::decode(decoder)?))
    }
}

impl BinaryEncode for ObjectHandle {
    fn encode(self, encoder: &mut EncodedData) {
        self.0.encode(encoder);
    }
}

impl super::EncodeTypeDef for ObjectHandle {
    fn encode_type_def(encoder: &mut super::TypeDef) {
        u32::encode_type_def(encoder);
    }
}
