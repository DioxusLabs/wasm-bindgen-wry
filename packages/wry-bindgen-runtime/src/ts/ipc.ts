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
 * - First u8: message type (0 = Evaluate, 1 = Respond, 2 = RespondError)
 * - Remaining data depends on message type
 */

import { DataDecoder, DataEncoder } from "./encoding";
import { getFunctionRegistry, getTypeCache, CachedTypeInfo } from "./function_registry";
import { parseTypeDef, TypeClass, HeapRefType } from "./types";

enum MessageType {
  Evaluate = 0,
  Respond = 1,
  RespondError = 2,
  // JS→Rust: an op in an Evaluate batch threw (a non-`catch` import). Rust
  // turns it into a panic so the Rust caller unwinds.
  RespondThrew = 3,
}

// Sentinel distinguishing "no op threw" from an op that threw `undefined`.
const NOT_THROWN = Symbol("not-thrown");

// Type caching markers - must match Rust's TYPE_CACHED and TYPE_FULL
const TYPE_CACHED = 0xff;
const TYPE_FULL = 0xfe;

// Reserved function ID for dropping native Rust refs - must match Rust's DROP_NATIVE_REF_FN_ID
const DROP_NATIVE_REF_FN_ID = 0xffffffff;

// Reserved function ID for calling exported Rust struct methods - must match Rust's CALL_EXPORT_FN_ID
const CALL_EXPORT_FN_ID = 0xfffffffe;

/**
 * Decode a base64 response body from a synchronous XHR.
 */
function decode_xhr_response(xhr: XMLHttpRequest): ArrayBuffer | null {
  if (xhr.status === 200 && xhr.responseText) {
    // Decode base64 response to ArrayBuffer
    const responseBinary = atob(xhr.responseText);
    const responseBytes = new Uint8Array(responseBinary.length);
    for (let i = 0; i < responseBinary.length; i++) {
      responseBytes[i] = responseBinary.charCodeAt(i);
    }
    return responseBytes.buffer;
  }
  return null;
}

/**
 * Sends binary data to Rust and receives binary response.
 */
function sync_request_binary(
  endpoint: string,
  data: ArrayBuffer
): ArrayBuffer | null {
  const xhr = new XMLHttpRequest();
  xhr.open("POST", endpoint, false);
  // Note: Cannot set responseType on sync requests - response comes as base64 text

  // Encode as base64 for header (Android workaround)
  const bytes = new Uint8Array(data);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  const base64 = btoa(binary);
  xhr.setRequestHeader("dioxus-data", base64);
  xhr.send();

  return decode_xhr_response(xhr);
}

function sync_lock_request(): ArrayBuffer | null {
  const xhr = new XMLHttpRequest();
  xhr.open("POST", `/__wbg__/handler`, false);
  xhr.setRequestHeader("wry-bindgen-lock", "1");
  xhr.send();
  return decode_xhr_response(xhr);
}

function sendEvaluateToRust(
  encodePayload: (encoder: DataEncoder) => void
): DataDecoder | null {
  const pendingHeapRefs = window.jsHeap.deferHeapRefs();
  const encoder = new DataEncoder(pendingHeapRefs);
  encoder.pushU8(MessageType.Evaluate);
  encodePayload(encoder);

  return handleBinaryResponse(
    sync_request_binary(`/__wbg__/handler`, encoder.finalize())
  );
}

/**
 * Entry point used by Rust to acquire the synchronous XHR lock. Rust may answer
 * the lock request with Evaluate messages; those are handled synchronously and
 * followed by nested XHRs until Rust releases the lock with an empty response.
 */
function acquire_handler_lock() {
  const remaining = handleBinaryResponse(sync_lock_request());
  if (remaining) {
    throw new Error("Unprocessed data remaining after lock handling");
  }
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

function takeIdList(decoder: DataDecoder): number[] {
  const count = decoder.takeU32();
  const ids: number[] = [];
  for (let i = 0; i < count; i++) {
    ids.push(decoder.takeU64());
  }
  return ids;
}

function installDeferredHeapRefs(decoder: DataDecoder): void {
  // A single install id-list rides in every prelude. An empty list means the
  // last inbound message carried no heap refs, so there is nothing to resolve.
  const ids = takeIdList(decoder);
  if (ids.length > 0) {
    window.jsHeap.resolveDeferredHeapRefs(ids);
  }
}

/**
 * Handle binary response from Rust.
 * May contain nested Evaluate calls (for callbacks).
 */
function handleBinaryResponse(
  response: ArrayBuffer | null
): DataDecoder | null {
  let currentResponse = response;

  while (currentResponse && currentResponse.byteLength !== 0) {
    const decoder = new DataDecoder(currentResponse);
    const msgType = decoder.takeU8();

    if (msgType === MessageType.Respond) {
      installDeferredHeapRefs(decoder);
      return decoder;
    } else if (msgType === MessageType.RespondError) {
      // Rust signalled that the call failed (e.g. an argument could not be
      // decoded). Surface it as a thrown exception the caller can catch,
      // matching wasm-bindgen's throw-on-bad-argument behavior.
      installDeferredHeapRefs(decoder);
      throw new Error(decoder.takeStr());
    } else if (msgType === MessageType.Evaluate) {
      installDeferredHeapRefs(decoder);

      // Read the explicit placeholder IDs Rust reserved for this batch.
      const reservedIds = takeIdList(decoder);
      window.jsHeap.pushReservationScope(reservedIds);

      const pendingHeapRefs = window.jsHeap.deferHeapRefs();
      const encoder = new DataEncoder(pendingHeapRefs);
      encoder.pushU8(MessageType.Respond);

      // Push a single borrow frame for this entire Evaluate message.
      // This frame persists across all operations and nested calls.
      window.jsHeap.pushBorrowFrame();

      // The borrow frame and reservation scope are popped in `finally` so a throw
      // inside the op loop cannot leave either stack desynced. On the error path
      // the reservation scope is popped without its fill-count check, so this
      // cleanup never throws over and masks the original (e.g. decode) error.
      let succeeded = false;
      let thrown: unknown = NOT_THROWN;
      try {
        while (decoder.hasMoreU32()) {
          const fnId = decoder.takeU32();
          const typeInfo = parseTypeInfo(decoder);

          const functionRegistry = getFunctionRegistry();
          const jsFunction = functionRegistry[fnId];
          if (!jsFunction) {
            throw new Error("Unknown function ID in response: " + fnId);
          }

          let params: unknown[];
          try {
            params = typeInfo.paramTypes.map((paramType) => paramType.decode(decoder));
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            const source = String(jsFunction).replace(/\s+/g, " ").slice(0, 160);
            throw new Error(
              `Failed to decode parameters for function ID ${fnId} (${source}): ${message}`
            );
          }

          const result = jsFunction(...params);

          if (typeInfo.returnType instanceof HeapRefType && reservedIds.length > 0) {
            window.jsHeap.fillNextReserved(result);
          } else {
            typeInfo.returnType.encode(encoder, result);
          }

          // Copy any `&mut [T]` arguments the import mutated back to Rust: the
          // mutated arrays follow the return value, in argument order, matching
          // the write-backs Rust queued while encoding the call.
          for (let i = 0; i < typeInfo.paramTypes.length; i++) {
            typeInfo.paramTypes[i].appendWriteBack?.(encoder, params[i]);
          }
        }
        succeeded = true;
      } catch (error) {
        // An op threw — a non-`catch` JS import propagating an exception. Report
        // it to Rust so the Rust caller unwinds (running destructors) instead of
        // letting the exception escape and desync the IPC.
        thrown = error;
      } finally {
        window.jsHeap.popBorrowFrame();
        window.jsHeap.popReservationScope(succeeded);
      }

      if (thrown !== NOT_THROWN) {
        const errEncoder = new DataEncoder(window.jsHeap.deferHeapRefs());
        errEncoder.pushU8(MessageType.RespondThrew);
        errEncoder.pushStr(
          thrown instanceof Error ? thrown.message : String(thrown)
        );
        currentResponse = sync_request_binary(
          `/__wbg__/handler`,
          errEncoder.finalize()
        );
        continue;
      }

      currentResponse = sync_request_binary(
        `/__wbg__/handler`,
        encoder.finalize()
      );
      continue;
    }

    if (!decoder.isEmpty()) {
      throw new Error("Unprocessed data remaining after Evaluate handling");
    }

    return null;
  }

  return null;
}

export {
  acquire_handler_lock,
  sendEvaluateToRust,
  DROP_NATIVE_REF_FN_ID,
  CALL_EXPORT_FN_ID,
};
