use alloc::vec::Vec;

use base64::Engine;
use wry_bindgen_abi::{BinaryDecode, DecodeError, unstable::DecodedDataBytes};

pub(crate) use wry_bindgen_abi::{DecodedData, EncodedData};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageType {
    Evaluate = 0,
    Respond = 1,
}

#[derive(Debug, Clone)]
pub(crate) struct IPCMessage {
    data: Vec<u8>,
}

impl IPCMessage {
    pub(crate) fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub(crate) fn decoded(&self) -> Result<DecodedVariant<'_>, DecodeError> {
        let mut decoded = DecodedData::from_bytes_unstable(&self.data)?;
        let message_type = u8::decode(&mut decoded)?;
        match message_type {
            0 => Ok(DecodedVariant::Evaluate { data: decoded }),
            1 => Ok(DecodedVariant::Respond { data: decoded }),
            value => Err(DecodeError::custom(format!(
                "invalid message type: {value}"
            ))),
        }
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }
}

pub(crate) enum DecodedVariant<'a> {
    Respond { data: DecodedData<'a> },
    Evaluate { data: DecodedData<'a> },
}

#[derive(Default)]
pub(crate) struct EncodedParts {
    u8_buf: Vec<u8>,
    u16_buf: Vec<u16>,
    u32_buf: Vec<u32>,
    str_buf: Vec<u8>,
}

impl EncodedParts {
    pub(crate) fn from_encoded(encoded: EncodedData) -> Self {
        Self::from_bytes(encoded.into_bytes())
    }

    pub(crate) fn append_encoded(&mut self, encoded: EncodedData) {
        self.append(Self::from_encoded(encoded));
    }

    pub(crate) fn append(&mut self, other: EncodedParts) {
        self.u8_buf.extend(other.u8_buf);
        self.u16_buf.extend(other.u16_buf);
        self.u32_buf.extend(other.u32_buf);
        self.str_buf.extend(other.str_buf);
    }

    pub(crate) fn into_message(self, message_type: MessageType, u32_prelude: &[u32]) -> IPCMessage {
        let mut u8_buf = Vec::with_capacity(1 + self.u8_buf.len());
        u8_buf.push(message_type as u8);
        u8_buf.extend(self.u8_buf);

        let mut u32_buf = Vec::with_capacity(u32_prelude.len() + self.u32_buf.len());
        u32_buf.extend_from_slice(u32_prelude);
        u32_buf.extend(self.u32_buf);

        IPCMessage::new(encode_sections(u8_buf, self.u16_buf, u32_buf, self.str_buf))
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        assert!(bytes.len() >= 12, "encoded data must contain a header");
        let u16_offset = read_u32(&bytes, 0) as usize;
        let u8_offset = read_u32(&bytes, 4) as usize;
        let str_offset = read_u32(&bytes, 8) as usize;
        assert!(
            12 <= u16_offset
                && u16_offset <= u8_offset
                && u8_offset <= str_offset
                && str_offset <= bytes.len(),
            "encoded data contains invalid section offsets"
        );

        let u32_buf = bytes[12..u16_offset]
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        let u16_buf = bytes[u16_offset..u8_offset]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        let u8_buf = bytes[u8_offset..str_offset].to_vec();
        let str_buf = bytes[str_offset..].to_vec();

        Self {
            u8_buf,
            u16_buf,
            u32_buf,
            str_buf,
        }
    }
}

#[cfg(test)]
pub(crate) fn empty_message(message_type: MessageType) -> IPCMessage {
    EncodedParts::default().into_message(message_type, &[])
}

pub(crate) fn decode_data(bytes: &[u8]) -> Option<IPCMessage> {
    let engine = base64::engine::general_purpose::STANDARD;
    let data = engine.decode(bytes).ok()?;
    Some(IPCMessage { data })
}

fn encode_sections(
    u8_buf: Vec<u8>,
    u16_buf: Vec<u16>,
    u32_buf: Vec<u32>,
    str_buf: Vec<u8>,
) -> Vec<u8> {
    let u16_offset = 12 + u32_buf.len() * 4;
    let u8_offset = u16_offset + u16_buf.len() * 2;
    let str_offset = u8_offset + u8_buf.len();
    let total_len = str_offset + str_buf.len();
    let mut bytes = Vec::with_capacity(total_len);

    bytes.extend_from_slice(&(u16_offset as u32).to_le_bytes());
    bytes.extend_from_slice(&(u8_offset as u32).to_le_bytes());
    bytes.extend_from_slice(&(str_offset as u32).to_le_bytes());
    for value in u32_buf {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in u16_buf {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&u8_buf);
    bytes.extend_from_slice(&str_buf);

    bytes
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
