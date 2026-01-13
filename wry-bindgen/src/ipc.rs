//! Binary IPC protocol types for communicating between Rust and JavaScript.
//!
//! The binary format uses aligned buffers for efficient memory access:
//! - First 12 bytes: three u32 offsets (u16_offset, u8_offset, str_offset)
//! - u32 buffer: from byte 12 to u16_offset
//! - u16 buffer: from u16_offset to u8_offset
//! - u8 buffer: from u8_offset to str_offset
//! - string buffer: from str_offset to end

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use base64::Engine;
use core::fmt;

/// Error type for decoding binary IPC messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The message is too short (less than 12 bytes for header)
    MessageTooShort { expected: usize, actual: usize },
    /// The u8 buffer is empty when trying to read
    U8BufferEmpty,
    /// The u16 buffer is empty when trying to read
    U16BufferEmpty,
    /// The u32 buffer is empty when trying to read
    U32BufferEmpty,
    /// The string buffer doesn't have enough bytes
    StringBufferTooShort { expected: usize, actual: usize },
    /// Invalid UTF-8 in string buffer
    InvalidUtf8 { position: usize },
    /// Invalid message type byte
    InvalidMessageType { value: u8 },
    /// Header offsets are invalid (e.g., overlapping or out of bounds)
    InvalidHeaderOffsets {
        u16_offset: u32,
        u8_offset: u32,
        str_offset: u32,
        total_len: usize,
    },
    /// Generic decode failure with context
    Custom(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::MessageTooShort { expected, actual } => {
                write!(
                    f,
                    "message too short: expected at least {expected} bytes, got {actual}"
                )
            }
            DecodeError::U8BufferEmpty => write!(f, "u8 buffer empty when trying to read"),
            DecodeError::U16BufferEmpty => write!(f, "u16 buffer empty when trying to read"),
            DecodeError::U32BufferEmpty => write!(f, "u32 buffer empty when trying to read"),
            DecodeError::StringBufferTooShort { expected, actual } => {
                write!(
                    f,
                    "string buffer too short: expected {expected} bytes, got {actual}"
                )
            }
            DecodeError::InvalidUtf8 { position } => {
                write!(f, "invalid UTF-8 at position {position}")
            }
            DecodeError::InvalidMessageType { value } => {
                write!(f, "invalid message type: {value}")
            }
            DecodeError::InvalidHeaderOffsets {
                u16_offset,
                u8_offset,
                str_offset,
                total_len,
            } => {
                write!(
                    f,
                    "invalid header offsets: u16={u16_offset}, u8={u8_offset}, str={str_offset}, total_len={total_len}"
                )
            }
            DecodeError::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

impl core::error::Error for DecodeError {}

impl From<DecodeError> for String {
    fn from(err: DecodeError) -> String {
        err.to_string()
    }
}

/// Message type identifier for IPC protocol.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageType {
    /// Rust calling JS (supports batching - multiple operations in one message)
    Evaluate = 0,
    /// JS/Rust responding to a call
    Respond = 1,
}

/// A binary IPC message.
///
/// Message format in the u8 buffer:
/// - First u8: message type (0 = Evaluate, 1 = Respond)
/// - Remaining data depends on message type
///
/// Evaluate format (supports batching - multiple operations in one message):
/// - u8: message type (0)
/// - For each operation (read until buffer exhausted):
///   - u32: function ID
///   - encoded arguments (varies by function)
///
/// Respond format:
/// - u8: message type (1)
/// - For each operation result:
///   - encoded return value (varies by function)
#[derive(Debug, Clone)]
pub(crate) struct IPCMessage {
    data: Vec<u8>,
}

impl IPCMessage {
    /// Create a new IPCMessage from raw bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create a new respond message with the given data.
    pub fn new_respond(push_data: impl FnOnce(&mut EncodedData)) -> Self {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Respond as u8);

        push_data(&mut encoder);

        IPCMessage::new(encoder.to_bytes())
    }

    /// Get the message type.
    pub fn ty(&self) -> Result<MessageType, DecodeError> {
        let mut decoded = DecodedData::from_bytes(&self.data)?;
        let message_type = decoded.take_u8()?;
        match message_type {
            0 => Ok(MessageType::Evaluate),
            1 => Ok(MessageType::Respond),
            v => Err(DecodeError::InvalidMessageType { value: v }),
        }
    }

    /// Decode the message into its variant form.
    pub fn decoded(&self) -> Result<DecodedVariant<'_>, DecodeError> {
        let mut decoded = DecodedData::from_bytes(&self.data)?;
        let message_type = decoded.take_u8()?;
        let message_type = match message_type {
            0 => DecodedVariant::Evaluate { data: decoded },
            1 => DecodedVariant::Respond { data: decoded },
            v => return Err(DecodeError::InvalidMessageType { value: v }),
        };
        Ok(message_type)
    }

    /// Get the raw data bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consume the message and return the raw data.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// Decoded message variant.
#[derive(Debug)]
pub(crate) enum DecodedVariant<'a> {
    /// Response from JS/Rust
    Respond { data: DecodedData<'a> },
    /// Evaluation request
    Evaluate { data: DecodedData<'a> },
}

/// Decoded binary data with aligned buffer access.
#[derive(Debug)]
pub struct DecodedData<'a> {
    u8_buf: &'a [u8],
    u16_buf: &'a [u16],
    u32_buf: &'a [u32],
    str_buf: &'a [u8],
}

impl<'a> DecodedData<'a> {
    /// Parse decoded data from raw bytes.
    pub(crate) fn from_bytes(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 12 {
            return Err(DecodeError::MessageTooShort {
                expected: 12,
                actual: bytes.len(),
            });
        }

        let header: [u32; 3] = bytemuck::cast_slice(&bytes[0..12])
            .try_into()
            .map_err(|_| DecodeError::Custom("failed to parse header".to_string()))?;
        let [u16_offset, u8_offset, str_offset] = header;

        // Validate offsets
        let total_len = bytes.len();
        if u16_offset as usize > total_len
            || u8_offset as usize > total_len
            || str_offset as usize > total_len
            || u16_offset < 12
            || u8_offset < u16_offset
            || str_offset < u8_offset
        {
            return Err(DecodeError::InvalidHeaderOffsets {
                u16_offset,
                u8_offset,
                str_offset,
                total_len,
            });
        }

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

    /// Take a u8 from the buffer.
    pub(crate) fn take_u8(&mut self) -> Result<u8, DecodeError> {
        let [first, rest @ ..] = &self.u8_buf else {
            return Err(DecodeError::U8BufferEmpty);
        };
        self.u8_buf = rest;
        Ok(*first)
    }

    /// Take a u16 from the buffer.
    pub(crate) fn take_u16(&mut self) -> Result<u16, DecodeError> {
        let [first, rest @ ..] = &self.u16_buf else {
            return Err(DecodeError::U16BufferEmpty);
        };
        self.u16_buf = rest;
        Ok(*first)
    }

    /// Take a u32 from the buffer.
    pub(crate) fn take_u32(&mut self) -> Result<u32, DecodeError> {
        let [first, rest @ ..] = &self.u32_buf else {
            return Err(DecodeError::U32BufferEmpty);
        };
        self.u32_buf = rest;
        Ok(*first)
    }

    /// Take a u64 from the buffer (stored as two u32s).
    pub(crate) fn take_u64(&mut self) -> Result<u64, DecodeError> {
        let low = self.take_u32()? as u64;
        let high = self.take_u32()? as u64;
        Ok((high << 32) | low)
    }

    /// Take a u128 from the buffer
    pub(crate) fn take_u128(&mut self) -> Result<u128, DecodeError> {
        let low = self.take_u64()? as u128;
        let high = self.take_u64()? as u128;
        Ok((high << 64) | low)
    }

    /// Take a string from the buffer.
    pub(crate) fn take_str(&mut self) -> Result<&'a str, DecodeError> {
        let len = self.take_u32()? as usize;
        let actual_len = self.str_buf.len();
        let Some((buf, rem)) = self.str_buf.split_at_checked(len) else {
            return Err(DecodeError::StringBufferTooShort {
                expected: len,
                actual: actual_len,
            });
        };
        let s = core::str::from_utf8(buf).map_err(|e| DecodeError::InvalidUtf8 {
            position: e.valid_up_to(),
        })?;
        self.str_buf = rem;
        Ok(s)
    }

    /// Check if the decoded data is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.u8_buf.is_empty()
            && self.u16_buf.is_empty()
            && self.u32_buf.is_empty()
            && self.str_buf.is_empty()
    }
}

/// Encoder for building binary messages.
#[derive(Debug, Default)]
pub struct EncodedData {
    pub(crate) u8_buf: Vec<u8>,
    pub(crate) u16_buf: Vec<u16>,
    pub(crate) u32_buf: Vec<u32>,
    pub(crate) str_buf: Vec<u8>,
    /// Flag indicating that this batch must be flushed before returning.
    /// Used for stack-allocated callbacks that need synchronous invocation.
    pub(crate) needs_flush: bool,
}

impl EncodedData {
    /// Create a new empty encoder.
    pub fn new() -> Self {
        Self {
            u8_buf: Vec::new(),
            u16_buf: Vec::new(),
            u32_buf: Vec::new(),
            str_buf: Vec::new(),
            needs_flush: false,
        }
    }

    /// Mark that this batch needs to be flushed before returning.
    /// Used for stack-allocated callbacks that require synchronous invocation.
    pub fn mark_needs_flush(&mut self) {
        self.needs_flush = true;
    }

    /// Get the total byte length of the encoded data.
    pub(crate) fn byte_len(&self) -> usize {
        12 + self.u32_buf.len() * 4
            + self.u16_buf.len() * 2
            + self.u8_buf.len()
            + self.str_buf.len()
    }

    /// Push a u8 to the buffer.
    pub(crate) fn push_u8(&mut self, value: u8) {
        self.u8_buf.push(value);
    }

    /// Push a u16 to the buffer.
    pub(crate) fn push_u16(&mut self, value: u16) {
        self.u16_buf.push(value);
    }

    /// Push a u32 to the buffer.
    pub(crate) fn push_u32(&mut self, value: u32) {
        self.u32_buf.push(value);
    }

    /// Prepend a u32 to the beginning of the buffer.
    /// Used to add the reserved placeholder count at the start of batch messages.
    pub fn prepend_u32(&mut self, value: u32) {
        self.u32_buf.insert(0, value);
    }

    /// Push a u64 to the buffer (stored as two u32s).
    pub(crate) fn push_u64(&mut self, value: u64) {
        self.push_u32((value & 0xFFFFFFFF) as u32);
        self.push_u32((value >> 32) as u32);
    }

    /// Push a u128 to the buffer
    pub(crate) fn push_u128(&mut self, value: u128) {
        self.push_u64((value & 0xFFFFFFFFFFFFFFFF) as u64);
        self.push_u64((value >> 64) as u64);
    }

    /// Push a string to the buffer.
    pub(crate) fn push_str(&mut self, value: &str) {
        self.push_u32(value.len() as u32);
        self.str_buf.extend_from_slice(value.as_bytes());
    }

    /// Convert the encoded data to bytes.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
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

    /// Extend this encoder with data from another encoder.
    pub(crate) fn extend(&mut self, other: &EncodedData) {
        self.u8_buf.extend_from_slice(&other.u8_buf);
        self.u16_buf.extend_from_slice(&other.u16_buf);
        self.u32_buf.extend_from_slice(&other.u32_buf);
        self.str_buf.extend_from_slice(&other.str_buf);
    }
}

/// Decode base64-encoded IPC data.
pub(crate) fn decode_data(bytes: &[u8]) -> Option<IPCMessage> {
    let engine = base64::engine::general_purpose::STANDARD;
    let data = engine.decode(bytes).ok()?;
    Some(IPCMessage { data })
}
