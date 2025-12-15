use base64::Engine;
use std::fmt::Debug;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Evaluate = 0,
    Respond = 1,
}

/// Binary protocol message types.
///
/// The binary format uses aligned buffers for efficient memory access:
/// - First 12 bytes: three u32 offsets (u16_offset, u8_offset, str_offset)
/// - u32 buffer: from byte 12 to u16_offset
/// - u16 buffer: from u16_offset to u8_offset  
/// - u8 buffer: from u8_offset to str_offset
/// - string buffer: from str_offset to end
///
/// Message format in the u8 buffer:
/// - First u8: message type (0 = Evaluate, 1 = Respond)
/// - Remaining data depends on message type
#[derive(Debug)]
pub(crate) struct IPCMessage {
    data: Vec<u8>,
}

impl IPCMessage {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn new_evaluate(fn_id: u32, push_args: impl FnOnce(&mut EncodedData)) -> Self {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Evaluate as u8);
        encoder.push_u32(fn_id);

        push_args(&mut encoder);

        IPCMessage::new(encoder.to_bytes())
    }

    pub fn new_respond(push_data: impl FnOnce(&mut EncodedData)) -> Self {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Respond as u8);

        push_data(&mut encoder);

        IPCMessage::new(encoder.to_bytes())
    }

    pub fn ty(&self) -> Result<MessageType, ()> {
        let mut decoded = DecodedData::from_bytes(&self.data)?;
        let message_type = decoded.take_u8()?;
        match message_type {
            0 => Ok(MessageType::Evaluate),
            1 => Ok(MessageType::Respond),
            _ => Err(()),
        }
    }

    pub fn decoded(&self) -> Result<DecodedVariant, ()> {
        let mut decoded = DecodedData::from_bytes(&self.data)?;
        let message_type = decoded.take_u8()?;
        let message_type = match message_type {
            0 => DecodedVariant::Evaluate {
                fn_id: decoded.take_u32()?,
                data: decoded,
            },
            1 => DecodedVariant::Respond {
                data: decoded,
            },
            _ => return Err(()),
        };
        Ok(message_type)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

pub enum DecodedVariant<'a> {
    Respond {
        data: DecodedData<'a>,
    },
    Evaluate {
        fn_id: u32,
        data: DecodedData<'a>,
    },
}

/// Decoded binary data with aligned buffer access
pub(crate) struct DecodedData<'a> {
    u8_buf: &'a [u8],
    u16_buf: &'a [u16],
    u32_buf: &'a [u32],
    str_buf: &'a [u8],
}

impl<'a> DecodedData<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() < 12 {
            return Err(());
        }

        let header: [u32; 3] = bytemuck::cast_slice(&bytes[0..12])
            .try_into()
            .map_err(|_| ())?;
        let [u16_offset, u8_offset, str_offset] = header;

        let u32_buf = bytemuck::cast_slice(&bytes[12..u16_offset as usize]);
        let u16_buf = bytemuck::cast_slice(&bytes[u16_offset as usize..u8_offset as usize]);
        let u8_buf = &bytes[u8_offset as usize..str_offset as usize];
        let str_buf = &bytes[str_offset as usize..];

        Ok(Self {
            u8_buf,
            u16_buf,
            u32_buf,
            str_buf,
        })
    }

    pub fn take_u8(&mut self) -> Result<u8, ()> {
        let [first, rest @ ..] = &self.u8_buf else {
            return Err(());
        };
        self.u8_buf = rest;
        Ok(*first)
    }

    pub fn take_u16(&mut self) -> Result<u16, ()> {
        let [first, rest @ ..] = &self.u16_buf else {
            return Err(());
        };
        self.u16_buf = rest;
        Ok(*first)
    }

    pub fn take_u32(&mut self) -> Result<u32, ()> {
        let [first, rest @ ..] = &self.u32_buf else {
            return Err(());
        };
        self.u32_buf = rest;
        Ok(*first)
    }
    pub fn take_u64(&mut self) -> Result<u64, ()> {
        let low = self.take_u32()? as u64;
        let high = self.take_u32()? as u64;
        Ok((high << 32) | low)
    }

    pub fn take_str(&mut self) -> Result<&'a str, ()> {
        let len = self.take_u32()? as usize;
        let Some((buf, rem)) = self.str_buf.split_at_checked(len) else {
            return Err(());
        };
        let s = std::str::from_utf8(buf).map_err(|_| ())?;
        self.str_buf = rem;
        Ok(s)
    }
}

/// Encoder for building binary messages
pub(crate) struct EncodedData {
    u8_buf: Vec<u8>,
    u16_buf: Vec<u16>,
    u32_buf: Vec<u32>,
    str_buf: Vec<u8>,
}

impl EncodedData {
    pub fn new() -> Self {
        Self {
            u8_buf: Vec::new(),
            u16_buf: Vec::new(),
            u32_buf: Vec::new(),
            str_buf: Vec::new(),
        }
    }

    pub fn push_u8(&mut self, value: u8) {
        self.u8_buf.push(value);
    }

    pub fn push_u16(&mut self, value: u16) {
        self.u16_buf.push(value);
    }

    pub fn push_u32(&mut self, value: u32) {
        self.u32_buf.push(value);
    }

    pub fn push_u64(&mut self, value: u64) {
        self.push_u32((value & 0xFFFFFFFF) as u32);
        self.push_u32((value >> 32) as u32);
    }

    pub fn push_str(&mut self, value: &str) {
        self.push_u32(value.len() as u32);
        self.str_buf.extend_from_slice(value.as_bytes());
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let u16_offset = 12 + self.u32_buf.len() * 4;
        let u8_offset = u16_offset + self.u16_buf.len() * 2;
        let str_offset = u8_offset + self.u8_buf.len();

        let total_len = str_offset + self.str_buf.len();
        let mut bytes = Vec::with_capacity(total_len);

        // Write header offsets
        bytes.extend_from_slice(&(u16_offset as u32).to_le_bytes());
        bytes.extend_from_slice(&(u8_offset as u32).to_le_bytes());
        bytes.extend_from_slice(&(str_offset as u32).to_le_bytes());

        // Write u32 buffer
        for &u in &self.u32_buf {
            bytes.extend_from_slice(&u.to_le_bytes());
        }

        // Write u16 buffer
        for &u in &self.u16_buf {
            bytes.extend_from_slice(&u.to_le_bytes());
        }

        // Write u8 buffer
        bytes.extend_from_slice(&self.u8_buf);

        // Write string buffer
        bytes.extend_from_slice(&self.str_buf);

        bytes
    }
}

pub(crate) fn decode_data(bytes: &[u8]) -> Option<IPCMessage> {
    // Decode base64 header
    let engine = base64::engine::general_purpose::STANDARD;
    let data = engine.decode(bytes).ok()?;

    Some(IPCMessage { data })
}
