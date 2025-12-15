import { RustFunction } from "./rust_function";

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

export { DataDecoder, DataEncoder };
