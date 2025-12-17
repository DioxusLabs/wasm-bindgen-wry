/**
 * Binary Protocol Encoder/Decoder
 *
 * The binary format uses aligned buffers for efficient memory access:
 * - First 12 bytes: three u32 offsets (u16_offset, u8_offset, str_offset)
 * - u32 buffer: from byte 12 to u16_offset
 * - u16 buffer: from u16_offset to u8_offset
 * - u8 buffer: from u8_offset to str_offset
 * - string buffer: from str_offset to end
 *
 * Message format in the u8 buffer:
 * - First u8: message type (0 = Evaluate, 1 = Respond)
 * - Remaining data depends on message type
 */

import { DataDecoder, DataEncoder } from "./encoding";

enum MessageType {
  Evaluate = 0,
  Respond = 1,
}

// Reserved function ID for dropping heap refs - must match Rust's DROP_HEAP_REF_FN_ID
const DROP_HEAP_REF_FN_ID = 0xFFFFFFFF;

// Reserved function ID for cloning heap refs - must match Rust's CLONE_HEAP_REF_FN_ID
const CLONE_HEAP_REF_FN_ID = 0xFFFFFFFE;

// Reserved function ID for dropping native Rust refs - must match Rust's DROP_NATIVE_REF_FN_ID
const DROP_NATIVE_REF_FN_ID = 0xFFFFFFFF;

/**
 * Sends binary data to Rust and receives binary response.
 */
function sync_request_binary(
  endpoint: string,
  data: ArrayBuffer
): ArrayBuffer | null {
  const xhr = new XMLHttpRequest();
  xhr.open("POST", endpoint, false);
  xhr.responseType = "arraybuffer";

  // Encode as base64 for header (Android workaround)
  const bytes = new Uint8Array(data);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  const base64 = btoa(binary);
  xhr.setRequestHeader("dioxus-data", base64);
  xhr.send();

  if (xhr.status === 200 && xhr.response && xhr.response.byteLength > 0) {
    return xhr.response as ArrayBuffer;
  }
  return null;
}




/**
 * Entry point for Rust to call JS functions using binary protocol.
 * Handles batched operations - reads and executes operations until buffer is exhausted.
 *
 * @param dataBase64 - Base64 encoded binary data containing message with operations
 */
function evaluate_from_rust_binary(dataBase64: string): unknown {
  // Decode base64 to ArrayBuffer
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return handleBinaryResponse(bytes.buffer);
}

/**
 * Handle binary response from Rust.
 * May contain nested Evaluate calls (for callbacks).
 */
function handleBinaryResponse(response: ArrayBuffer | null): unknown {
  if (!response || response.byteLength === 0) {
    return undefined;
  }

  const decoder = new DataDecoder(response);
  const rawMsgType = decoder.takeU8();
  const msgType: MessageType = rawMsgType;

  if (msgType === MessageType.Respond) {
    // Respond - just return (caller will decode the value)
    return undefined;
  } else if (msgType === MessageType.Evaluate) {
    // Evaluate - Rust is calling JS functions (possibly multiple)

    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Respond);

    // Process all operations
    while (decoder.hasMoreU32()) {
      const fnId = decoder.takeU32();

      // Handle special drop function
      if (fnId === DROP_HEAP_REF_FN_ID) {
        const heapId = decoder.takeU64();
        window.jsHeap.remove(heapId);
        continue;
      }

      // Handle special clone function
      if (fnId === CLONE_HEAP_REF_FN_ID) {
        const heapId = decoder.takeU64();
        const value = window.jsHeap.get(heapId);
        const newId = window.jsHeap.insert(value);
        encoder.pushU64(newId);
        continue;
      }

      const spec = window.functionRegistry[fnId];
      if (!spec) {
        throw new Error("Unknown function ID in response: " + fnId);
      }

      spec(decoder, encoder);
    }

    const nextResponse = sync_request_binary(
      "wry://handler",
      encoder.finalize()
    );
    return handleBinaryResponse(nextResponse);
  }

  return undefined;
}

export { evaluate_from_rust_binary, handleBinaryResponse, sync_request_binary, MessageType, DROP_NATIVE_REF_FN_ID };