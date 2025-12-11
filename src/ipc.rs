use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

// Message types for thread communication
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum IPCMessage {
    Evaluate {
        fn_id: u64,
        args: Vec<serde_json::Value>,
    },
    Respond {
        response: serde_json::Value,
    },
    Shutdown,
}

pub(crate) fn decode_data(bytes: &[u8]) -> Option<IPCMessage> {
    // Decode base64 header
    let engine = base64::engine::general_purpose::STANDARD;
    if let Ok(decoded_bytes) = engine.decode(bytes) {
        return serde_json::from_slice(&decoded_bytes).ok();
    }
    None
}
