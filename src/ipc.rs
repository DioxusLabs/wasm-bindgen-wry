use base64::Engine;
use std::fmt::Debug;

/// Binary protocol message types.
/// 
/// The binary format uses aligned buffers for efficient memory access:
/// - First 12 bytes: three u32 offsets (u16_offset, u8_offset, str_offset)
/// - u32 buffer: from byte 12 to u16_offset
/// - u16 buffer: from u16_offset to u8_offset  
/// - u8 buffer: from u8_offset to str_offset
/// - string buffer: from str_offset to end
///
/// Message format in the u32 buffer:
/// - First u32: message type (0 = Evaluate, 1 = Respond)
/// - Remaining data depends on message type
#[derive(Debug)]
pub(crate) enum IPCMessage {
    /// Evaluate a JS function
    /// Binary format: [0, fn_id] followed by serialized args
    Evaluate {
        fn_id: u32,
        data: Vec<u8>,
    },
    /// Response from JS
    /// Binary format: [1] followed by serialized response
    Respond {
        data: Vec<u8>,
    },
    #[allow(dead_code)]
    Shutdown,
}

/// Decoded binary data with aligned buffer access
pub(crate) struct DecodedData<'a> {
    u8_buf: &'a [u8],
    u8_offset: usize,
    u16_buf: &'a [u16],
    u16_offset: usize,
    u32_buf: &'a [u32],
    u32_offset: usize,
    str_buf: &'a [u8],
    str_offset: usize,
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
            u8_offset: 0,
            u16_buf,
            u16_offset: 0,
            u32_buf,
            u32_offset: 0,
            str_buf,
            str_offset: 0,
        })
    }

    pub fn take_u8(&mut self) -> Result<u8, ()> {
        if self.u8_offset >= self.u8_buf.len() {
            return Err(());
        }
        let val = self.u8_buf[self.u8_offset];
        self.u8_offset += 1;
        Ok(val)
    }

    pub fn take_u16(&mut self) -> Result<u16, ()> {
        if self.u16_offset >= self.u16_buf.len() {
            return Err(());
        }
        let val = self.u16_buf[self.u16_offset];
        self.u16_offset += 1;
        Ok(val)
    }

    pub fn take_u32(&mut self) -> Result<u32, ()> {
        if self.u32_offset >= self.u32_buf.len() {
            return Err(());
        }
        let val = self.u32_buf[self.u32_offset];
        self.u32_offset += 1;
        Ok(val)
    }

    pub fn take_u64(&mut self) -> Result<u64, ()> {
        let low = self.take_u32()? as u64;
        let high = self.take_u32()? as u64;
        Ok((high << 32) | low)
    }

    pub fn take_str(&mut self) -> Result<&'a str, ()> {
        let len = self.take_u32()? as usize;
        if self.str_offset + len > self.str_buf.len() {
            return Err(());
        }
        let s = std::str::from_utf8(&self.str_buf[self.str_offset..self.str_offset + len])
            .map_err(|_| ())?;
        self.str_offset += len;
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
    let decoded_bytes = engine.decode(bytes).ok()?;
    
    if decoded_bytes.len() < 12 {
        return None;
    }
    
    let mut decoded = DecodedData::from_bytes(&decoded_bytes).ok()?;
    let msg_type = decoded.take_u32().ok()?;
    
    match msg_type {
        0 => {
            // Evaluate: fn_id followed by args data
            let fn_id = decoded.take_u32().ok()?;
            Some(IPCMessage::Evaluate {
                fn_id,
                data: decoded_bytes,
            })
        }
        1 => {
            // Respond: just the response data
            Some(IPCMessage::Respond {
                data: decoded_bytes,
            })
        }
        _ => None,
    }
}

pub(crate) fn encode_evaluate(fn_id: u32, args_data: &EncodedData) -> Vec<u8> {
    let mut encoder = EncodedData::new();
    encoder.push_u32(0); // Evaluate message type
    encoder.push_u32(fn_id);
    
    // Merge the args data
    encoder.u8_buf.extend_from_slice(&args_data.u8_buf);
    encoder.u16_buf.extend_from_slice(&args_data.u16_buf);
    encoder.u32_buf.extend_from_slice(&args_data.u32_buf);
    encoder.str_buf.extend_from_slice(&args_data.str_buf);
    
    encoder.to_bytes()
}

pub(crate) fn encode_respond(response_data: &EncodedData) -> Vec<u8> {
    let mut encoder = EncodedData::new();
    encoder.push_u32(1); // Respond message type
    
    // Merge the response data
    encoder.u8_buf.extend_from_slice(&response_data.u8_buf);
    encoder.u16_buf.extend_from_slice(&response_data.u16_buf);
    encoder.u32_buf.extend_from_slice(&response_data.u32_buf);
    encoder.str_buf.extend_from_slice(&response_data.str_buf);
    
    encoder.to_bytes()
}
