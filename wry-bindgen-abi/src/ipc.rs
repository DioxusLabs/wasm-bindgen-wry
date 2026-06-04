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
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    message: String,
}

impl DecodeError {
    pub fn custom(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub(crate) fn message_too_short(expected: usize, actual: usize) -> Self {
        Self::custom(
            format_args!("message too short: expected at least {expected} bytes, got {actual}")
                .to_string(),
        )
    }

    pub(crate) fn u8_buffer_empty() -> Self {
        Self::custom("u8 buffer empty when trying to read")
    }

    pub(crate) fn u16_buffer_empty() -> Self {
        Self::custom("u16 buffer empty when trying to read")
    }

    pub(crate) fn u32_buffer_empty() -> Self {
        Self::custom("u32 buffer empty when trying to read")
    }

    pub(crate) fn string_buffer_too_short(expected: usize, actual: usize) -> Self {
        Self::custom(
            format_args!("string buffer too short: expected {expected} bytes, got {actual}")
                .to_string(),
        )
    }

    pub(crate) fn invalid_utf8(position: usize) -> Self {
        Self::custom(format_args!("invalid UTF-8 at position {position}").to_string())
    }

    pub(crate) fn invalid_header_offsets(
        u16_offset: u32,
        u8_offset: u32,
        str_offset: u32,
        total_len: usize,
    ) -> Self {
        Self::custom(format_args!(
            "invalid header offsets: u16={u16_offset}, u8={u8_offset}, str={str_offset}, total_len={total_len}"
        ).to_string())
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl core::error::Error for DecodeError {}

impl From<DecodeError> for String {
    fn from(err: DecodeError) -> String {
        err.to_string()
    }
}

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
            return Err(DecodeError::message_too_short(12, bytes.len()));
        }

        let header: [u32; 3] = bytemuck::cast_slice(&bytes[0..12])
            .try_into()
            .map_err(|_| DecodeError::custom("failed to parse header"))?;
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
            return Err(DecodeError::invalid_header_offsets(
                u16_offset, u8_offset, str_offset, total_len,
            ));
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
            return Err(DecodeError::u8_buffer_empty());
        };
        self.u8_buf = rest;
        Ok(*first)
    }

    /// Take a u16 from the buffer.
    pub(crate) fn take_u16(&mut self) -> Result<u16, DecodeError> {
        let [first, rest @ ..] = &self.u16_buf else {
            return Err(DecodeError::u16_buffer_empty());
        };
        self.u16_buf = rest;
        Ok(*first)
    }

    /// Take a u32 from the buffer.
    pub(crate) fn take_u32(&mut self) -> Result<u32, DecodeError> {
        let [first, rest @ ..] = &self.u32_buf else {
            return Err(DecodeError::u32_buffer_empty());
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
            return Err(DecodeError::string_buffer_too_short(len, actual_len));
        };
        let s =
            core::str::from_utf8(buf).map_err(|e| DecodeError::invalid_utf8(e.valid_up_to()))?;
        self.str_buf = rem;
        Ok(s)
    }
}

#[derive(Default)]
pub struct EncodedData {
    u8_buf: Vec<u8>,
    u16_buf: Vec<u16>,
    u32_buf: Vec<u32>,
    str_buf: Vec<u8>,
    /// Flag indicating that this batch must be flushed before returning.
    /// Used for stack-allocated callbacks that need synchronous invocation.
    needs_flush: bool,
}

impl EncodedData {
    #[doc(hidden)]
    pub fn mark_needs_flush(&mut self) {
        self.needs_flush = true;
    }

    pub fn take_needs_flush(&mut self) -> bool {
        core::mem::take(&mut self.needs_flush)
    }

    pub(crate) fn push_u8(&mut self, value: u8) {
        self.u8_buf.push(value);
    }

    pub(crate) fn push_u16(&mut self, value: u16) {
        self.u16_buf.push(value);
    }

    pub(crate) fn push_u32(&mut self, value: u32) {
        self.u32_buf.push(value);
    }

    pub(crate) fn push_u64(&mut self, value: u64) {
        self.push_u32((value & 0xFFFFFFFF) as u32);
        self.push_u32((value >> 32) as u32);
    }

    pub(crate) fn push_u128(&mut self, value: u128) {
        self.push_u64((value & 0xFFFFFFFFFFFFFFFF) as u64);
        self.push_u64((value >> 64) as u64);
    }

    pub(crate) fn push_str(&mut self, value: &str) {
        let len = u32::try_from(value.len()).expect("string length exceeds u32::MAX");
        self.push_u32(len);
        self.str_buf.extend_from_slice(value.as_bytes());
    }

    /// Convert the encoded data to bytes.
    pub fn into_bytes(self) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_data_roundtrips_buffer_sections() {
        let mut encoder = EncodedData::default();
        encoder.push_u8(7);
        encoder.push_u32(99);

        let bytes = encoder.into_bytes();
        let mut data = DecodedData::from_bytes(&bytes).unwrap();
        assert_eq!(data.take_u8().unwrap(), 7);
        assert_eq!(data.take_u32().unwrap(), 99);
    }

    #[test]
    fn flush_flag_is_consumed_once() {
        let mut encoder = EncodedData::default();
        encoder.mark_needs_flush();
        assert!(encoder.take_needs_flush());
        assert!(!encoder.take_needs_flush());
    }
}
