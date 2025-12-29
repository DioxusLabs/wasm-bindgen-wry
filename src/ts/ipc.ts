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
import { getFunctionRegistry, getTypeCache, CachedTypeInfo } from "./function_registry";
import { parseTypeDef, TypeClass } from "./types";

enum MessageType {
  Evaluate = 0,
  Respond = 1,
}

// Type caching markers - must match Rust's TYPE_CACHED and TYPE_FULL
const TYPE_CACHED = 0xff;
const TYPE_FULL = 0xfe;

// Reserved function ID for dropping heap refs - must match Rust's DROP_HEAP_REF_FN_ID
const DROP_HEAP_REF_FN_ID = 0xffffffff;

// Reserved function ID for cloning heap refs - must match Rust's CLONE_HEAP_REF_FN_ID
const CLONE_HEAP_REF_FN_ID = 0xfffffffe;

// Reserved function ID for dropping native Rust refs - must match Rust's DROP_NATIVE_REF_FN_ID
const DROP_NATIVE_REF_FN_ID = 0xffffffff;

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
function evaluate_from_rust_binary(dataBase64: string): DataDecoder | null {
  // Decode base64 to ArrayBuffer
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return handleBinaryResponse(bytes.buffer);
}

/**
 * Parse type information from the decoder.
 * Handles both cached and full type definitions.
 */
function parseTypeInfo(decoder: DataDecoder): CachedTypeInfo {
  const typeCache = getTypeCache();
  const typeMarker = decoder.takeU8();

  if (typeMarker === TYPE_CACHED) {
    // Cached type - look up by ID
    const typeId = decoder.takeU32();
    const cached = typeCache.get(typeId);
    if (!cached) {
      throw new Error(`Unknown cached type ID: ${typeId}`);
    }
    return cached;
  } else if (typeMarker === TYPE_FULL) {
    // Full type definition - parse and cache
    const typeId = decoder.takeU32();
    const paramCount = decoder.takeU8();

    // Get the remaining bytes for parsing type definitions
    const typeBytes = decoder.getRemainingBytes();
    const offset = { value: 0 };

    const paramTypes: TypeClass[] = [];
    for (let i = 0; i < paramCount; i++) {
      paramTypes.push(parseTypeDef(typeBytes, offset));
    }
    const returnType = parseTypeDef(typeBytes, offset);

    // Advance the decoder past the type definition bytes we consumed
    decoder.skipBytes(offset.value);

    const cached: CachedTypeInfo = { paramTypes, returnType };
    typeCache.set(typeId, cached);
    return cached;
  } else {
    throw new Error(`Unknown type marker: ${typeMarker}`);
  }
}

/**
 * Handle binary response from Rust.
 * May contain nested Evaluate calls (for callbacks).
 */
function handleBinaryResponse(
  response: ArrayBuffer | null
): DataDecoder | null {
  if (!response || response.byteLength === 0) {
    return null;
  }

  const decoder = new DataDecoder(response);
  const rawMsgType = decoder.takeU8();
  const msgType: MessageType = rawMsgType;

  if (msgType === MessageType.Respond) {
    // Respond - just return the decoder for further processing
    return decoder;
  } else if (msgType === MessageType.Evaluate) {
    // Evaluate - Rust is calling JS functions (possibly multiple)

    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Respond);

    // Process all operations
    while (decoder.hasMoreU32()) {
      const fnId = decoder.takeU32();
      // Parse type information (cached or full)
      const typeInfo = parseTypeInfo(decoder);

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

      // Get the raw JS function
      const functionRegistry = getFunctionRegistry();
      const jsFunction = functionRegistry[fnId];
      if (!jsFunction) {
        throw new Error("Unknown function ID in response: " + fnId);
      }

      // Decode parameters using their respective types
      const params = typeInfo.paramTypes.map((paramType) => paramType.decode(decoder));
      console.log("[IPC] Calling JS function ID:", fnId, "with params:", params);

      // Call the original JS function with decoded parameters
      const result = jsFunction(...params);

      console.log("[IPC] JS function ID:", fnId, "returned:", result);
      console.log("[IPC] Encoding result with return type:", typeInfo.returnType);

      // Encode the result using the return type
      typeInfo.returnType.encode(encoder, result);
    }

    const nextResponse = sync_request_binary(
      "wry://handler",
      encoder.finalize()
    );
    return handleBinaryResponse(nextResponse);
  }

  return null;
}

export {
  evaluate_from_rust_binary,
  handleBinaryResponse,
  sync_request_binary,
  MessageType,
  DROP_NATIVE_REF_FN_ID,
};
