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

enum MessageType {
  Evaluate = 0,
  Respond = 1,
}

/**
 * Encoder for building binary messages to send to Rust.
 */
class DataEncoder {
  private u8Buf: number[];
  private u16Buf: number[];
  private u32Buf: number[];
  private strBuf: number[]; // UTF-8 bytes

  constructor() {
    this.u8Buf = [];
    this.u16Buf = [];
    this.u32Buf = [];
    this.strBuf = [];
  }

  pushNull() {}

  pushBool(value: boolean) {
    this.u8Buf.push(value ? 1 : 0);
  }

  pushHeapRef(obj: unknown) {
    const id = jsHeap.insert(obj);
    this.pushU64(id);
  }

  pushU8(value: number) {
    this.u8Buf.push(value & 0xff);
  }

  pushU16(value: number) {
    this.u16Buf.push(value & 0xffff);
  }

  pushU32(value: number) {
    this.u32Buf.push(value >>> 0);
  }

  pushU64(value: number) {
    const low = value >>> 0;
    const high = Math.floor(value / 0x100000000) >>> 0;
    this.pushU32(low);
    this.pushU32(high);
  }

  pushStr(value: string) {
    const encoded = new TextEncoder().encode(value);
    this.pushU32(encoded.length);
    for (let i = 0; i < encoded.length; i++) {
      this.strBuf.push(encoded[i]);
    }
  }

  finalize(): ArrayBuffer {
    const u16Offset = 12 + this.u32Buf.length * 4;
    const u8Offset = u16Offset + this.u16Buf.length * 2;
    const strOffset = u8Offset + this.u8Buf.length;
    const totalSize = strOffset + this.strBuf.length;

    const buffer = new ArrayBuffer(totalSize);
    const dataView = new DataView(buffer);

    // Write header offsets (little-endian)
    dataView.setUint32(0, u16Offset, true);
    dataView.setUint32(4, u8Offset, true);
    dataView.setUint32(8, strOffset, true);

    // Write u32 buffer
    let offset = 12;
    for (const val of this.u32Buf) {
      dataView.setUint32(offset, val, true);
      offset += 4;
    }

    // Write u16 buffer
    for (const val of this.u16Buf) {
      dataView.setUint16(offset, val, true);
      offset += 2;
    }

    // Write u8 buffer
    const u8View = new Uint8Array(buffer, u8Offset, this.u8Buf.length);
    u8View.set(this.u8Buf);

    // Write string buffer
    const strView = new Uint8Array(buffer, strOffset, this.strBuf.length);
    strView.set(this.strBuf);

    return buffer;
  }
}

/**
 * Decoder for reading binary messages from Rust.
 */
class DataDecoder {
  private u8Buf: Uint8Array;
  private u8Offset: number;

  private u16Buf: Uint16Array;
  private u16Offset: number;

  private u32Buf: Uint32Array;
  private u32Offset: number;

  private strBuf: Uint8Array;
  private strOffset: number;

  constructor(data: ArrayBuffer) {
    const headerView = new DataView(data, 0, 12);
    const u16ByteOffset = headerView.getUint32(0, true);
    const u8ByteOffset = headerView.getUint32(4, true);
    const strByteOffset = headerView.getUint32(8, true);

    // u32 buffer starts at byte 12, ends at u16ByteOffset
    const u32ByteLength = u16ByteOffset - 12;
    this.u32Buf = new Uint32Array(data, 12, u32ByteLength / 4);
    this.u32Offset = 0;

    // u16 buffer
    const u16ByteLength = u8ByteOffset - u16ByteOffset;
    this.u16Buf = new Uint16Array(data, u16ByteOffset, u16ByteLength / 2);
    this.u16Offset = 0;

    // u8 buffer
    const u8ByteLength = strByteOffset - u8ByteOffset;
    this.u8Buf = new Uint8Array(data, u8ByteOffset, u8ByteLength);
    this.u8Offset = 0;

    // string buffer
    this.strBuf = new Uint8Array(data, strByteOffset);
    this.strOffset = 0;
  }

  takeNull(): null {
    return null;
  }

  takeBool(): boolean {
    const val = this.takeU8();
    return val !== 0;
  }

  takeHeapRef(): unknown {
    const id = this.takeU64();
    return jsHeap.get(id);
  }

  takeRustCallback(): () => unknown {
    const fnId = this.takeU64();
    const f = new RustFunction(fnId);
    return () => f.call();
  }

  takeU8(): number {
    return this.u8Buf[this.u8Offset++];
  }

  takeU16(): number {
    return this.u16Buf[this.u16Offset++];
  }

  takeU32(): number {
    return this.u32Buf[this.u32Offset++];
  }

  takeU64(): number {
    const low = this.takeU32();
    const high = this.takeU32();
    return low + high * 0x100000000;
  }

  takeStr(): string {
    const len = this.takeU32();
    const bytes = this.strBuf.subarray(this.strOffset, this.strOffset + len);
    this.strOffset += len;
    return new TextDecoder("utf-8").decode(bytes);
  }
}

// SlotMap implementation for JS heap types
class JSHeap {
  private slots: (unknown | undefined)[];
  private freeIds: number[];
  private maxId: number;

  constructor() {
    this.slots = [];
    this.freeIds = [];
    this.maxId = 0;
  }

  insert(value: unknown): number {
    let id: number;
    if (this.freeIds.length > 0) {
      id = this.freeIds.pop()!;
    } else {
      id = this.maxId;
      this.maxId++;
    }
    this.slots[id] = value;
    return id;
  }

  get(id: number): unknown | undefined {
    return this.slots[id];
  }

  remove(id: number): unknown | undefined {
    const value = this.slots[id];
    if (value !== undefined) {
      this.slots[id] = undefined;
      this.freeIds.push(id);
    }
    return value;
  }

  has(id: number): boolean {
    return this.slots[id] !== undefined;
  }
}

const jsHeap = new JSHeap();

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
 * Rust function wrapper that can call back into Rust.
 */
class RustFunction {
  private fnId: number;

  constructor(fnId: number) {
    this.fnId = fnId;
  }

  call(): unknown {
    // Build Evaluate message: [0, fn_id]
    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Evaluate);
    encoder.pushU32(0); // Call argument function
    encoder.pushU64(this.fnId);

    const response = sync_request_binary("wry://handler", encoder.finalize());
    return handleBinaryResponse(response);
  }
}

/**
 * Function registry - maps function IDs to their serialization/deserialization specs.
 *
 * Each function has:
 * - Argument deserialization: how to read args from decoder
 * - Return serialization: how to write return value to encoder
 */
type FunctionSpec = (decoder: DataDecoder, encoder: DataEncoder) => void;

let functionRegistry: FunctionSpec[] = [];

/**
 * Entry point for Rust to call JS functions using binary protocol.
 *
 * @param fnId - The function ID to call
 * @param dataBase64 - Base64 encoded binary data containing message
 */
function evaluate_from_rust_binary(fnId: number, dataBase64: string): unknown {
  // Decode base64 to ArrayBuffer
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  const data = bytes.buffer;

  // Decode the message
  const decoder = new DataDecoder(data);
  const msgType = decoder.takeU8(); // Should be 0 (Evaluate)
  const decodedFnId = decoder.takeU32(); // Function ID

  const spec = functionRegistry[fnId];
  if (!spec) {
    throw new Error("Unknown function ID: " + fnId);
  }

  const encoder = new DataEncoder();
  encoder.pushU8(MessageType.Respond);
  spec(decoder, encoder);

  const response = sync_request_binary("wry://handler", encoder.finalize());
  return handleBinaryResponse(response);
}

/**
 * Handle binary response from Rust.
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
    // Evaluate - Rust is calling a JS function
    const fnId = decoder.takeU32();

    const spec = functionRegistry[fnId];
    if (!spec) {
      throw new Error("Unknown function ID in response: " + fnId);
    }

    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Respond);
    spec(decoder, encoder);

    const nextResponse = sync_request_binary(
      "wry://handler",
      encoder.finalize()
    );
    return handleBinaryResponse(nextResponse);
  }

  return undefined;
}

// @ts-ignore
window.evaluate_from_rust_binary = evaluate_from_rust_binary;
// @ts-ignore
window.jsHeap = jsHeap;
// @ts-ignore
window.setFunctionRegistry = (registry: FunctionSpec[]) => {
  functionRegistry = registry;
};

