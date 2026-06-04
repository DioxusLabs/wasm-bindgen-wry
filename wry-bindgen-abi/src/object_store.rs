use crate::{BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodedData};

#[derive(Clone, Copy)]
pub struct ObjectHandle(u32);

impl ObjectHandle {
    pub(crate) const fn from_raw_inner(raw: u32) -> Self {
        Self(raw)
    }

    pub(crate) fn raw_inner(self) -> u32 {
        self.0
    }
}

impl BinaryDecode for ObjectHandle {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        Ok(ObjectHandle::from_raw_inner(u32::decode(decoder)?))
    }
}

impl BinaryEncode for ObjectHandle {
    fn encode(self, encoder: &mut EncodedData) {
        self.0.encode(encoder);
    }
}

impl crate::EncodeTypeDef for ObjectHandle {
    fn encode_type_def(encoder: &mut crate::TypeDef) {
        u32::encode_type_def(encoder);
    }
}
