import { DataEncoder, DataDecoder } from "./encoding";
import { RustFunction } from "./rust_function";

/**
 * Type tags for the binary type definition protocol.
 * Must match the Rust TypeTag enum exactly.
 */
enum TypeTag {
  Null = 0,
  Bool = 1,
  U8 = 2,
  U16 = 3,
  U32 = 4,
  U64 = 5,
  U128 = 6,
  I8 = 7,
  I16 = 8,
  I32 = 9,
  I64 = 10,
  I128 = 11,
  F32 = 12,
  F64 = 13,
  Usize = 14,
  Isize = 15,
  String = 16,
  HeapRef = 17,
  Callback = 18,
  Option = 19,
  Result = 20,
  Array = 21,
  BorrowedRef = 22,
  U8Clamped = 23,
  StringEnum = 24,
}

/**
 * Base interface for all type classes
 */
interface TypeClass {
  encode(encoder: DataEncoder, value: any): void;
  decode(decoder: DataDecoder): any;
}

/**
 * Type class for boolean values with encoding/decoding methods
 */
class BoolType implements TypeClass {
  encode(encoder: DataEncoder, value: boolean): void {
    encoder.pushU8(value ? 1 : 0);
  }

  decode(decoder: DataDecoder): boolean {
    const val = decoder.takeU8();
    return val !== 0;
  }
}

/**
 * Type class for heap references with encoding/decoding methods
 */
class HeapRefType implements TypeClass {
  encode(encoder: DataEncoder, obj: unknown): void {
    // Insert into heap but don't encode the id - Rust side is in sync with the slab
    window.jsHeap.insert(obj);
  }

  decode(decoder: DataDecoder): unknown {
    const id = decoder.takeU64();
    return window.jsHeap.get(id);
  }
}

/**
 * Type class for borrowed references with encoding/decoding methods.
 * Borrowed references use the borrow stack (indices 1-127) instead of the heap.
 * They are automatically cleaned up after each operation completes.
 */
class BorrowedRefType implements TypeClass {
  encode(encoder: DataEncoder, obj: unknown): void {
    // Put on borrow stack instead of heap - ID is not encoded, Rust side syncs via batch state
    window.jsHeap.addBorrowedRef(obj);
  }

  decode(decoder: DataDecoder): unknown {
    const id = decoder.takeU64();
    // Works for both heap refs (128+) and borrow stack refs (1-127)
    return window.jsHeap.get(id);
  }
}

/**
 * Type class for string values with encoding/decoding methods
 */
class StringType implements TypeClass {
  encode(encoder: DataEncoder, value: string): void {
    encoder.pushStr(value);
  }

  decode(decoder: DataDecoder): string {
    return decoder.takeStr();
  }
}

/**
 * Type class for string enum values with u32 encoding and lookup arrays
 */
class StringEnumType implements TypeClass {
  private lookupArray: string[];

  constructor(lookupArray: string[]) {
    this.lookupArray = lookupArray;
  }

  encode(encoder: DataEncoder, value: string): void {
    const index = this.lookupArray.indexOf(value);
    // Invalid values encoded as lookupArray.length (maps to __Invalid variant)
    const encoded = index >= 0 ? index : this.lookupArray.length;
    encoder.pushU32(encoded);
  }

  decode(decoder: DataDecoder): string {
    const index = decoder.takeU32();
    return this.lookupArray[index];
  }
}

/**
 * Type class for Rust callbacks with encoding/decoding methods
 */
class CallbackType implements TypeClass {
  private paramTypes: TypeClass[];
  private returnType: TypeClass;

  constructor(paramTypes: TypeClass[], returnType: TypeClass) {
    this.paramTypes = paramTypes;
    this.returnType = returnType;
  }

  encode(encoder: DataEncoder, fnId: number): void {
    encoder.pushU32(fnId);
  }

  decode(decoder: DataDecoder): (...args: any[]) => any {
    const fnId = decoder.takeU32();
    const f = new RustFunction(fnId, this.paramTypes, this.returnType);
    return (...args: any[]) => f.call(...args);
  }
}

/**
 * Type class for null values with encoding/decoding methods
 */
class NullType implements TypeClass {
  encode(encoder: DataEncoder, value: null): void {
    // Null doesn't need to encode anything
  }

  decode(decoder: DataDecoder): null {
    return null;
  }
}

type NumberType = "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128" | "usize" | "isize" | "f32" | "f64";

/**
 * Type class for numeric values (u8, u16, u32, u64, i8, i16, i32, i64, usize, isize, f32, f64) with encoding/decoding methods
 */
class NumericType implements TypeClass {
  private size: NumberType;

  constructor(size: NumberType) {
    this.size = size;
  }

  encode(encoder: DataEncoder, value: number): void {
    switch (this.size) {
      case "u8":
        encoder.pushU8(value);
        break;
      case "u16":
        encoder.pushU16(value);
        break;
      case "u32":
        encoder.pushU32(value);
        break;
      case "u64":
        encoder.pushU64(value);
        break;
      case "u128":
        encoder.pushU128(value);
        break;
      case "i8":
        // Signed integers encode as unsigned (Rust: self as u8)
        encoder.pushU8(value & 0xff);
        break;
      case "i16":
        // Signed integers encode as unsigned (Rust: self as u16)
        encoder.pushU16(value & 0xffff);
        break;
      case "i32":
        // Signed integers encode as unsigned (Rust: self as u32)
        encoder.pushU32(value >>> 0);
        break;
      case "i64":
        // Signed integers encode as unsigned (Rust: self as u64)
        encoder.pushU64(value);
        break;
      case "i128":
        // Signed integers encode as unsigned (Rust: self as u128)
        encoder.pushU128(value);
        break;
      case "usize":
        // usize encodes as u64
        encoder.pushU64(value);
        break;
      case "isize":
        // isize encodes as u64 (Rust: self as u64)
        encoder.pushU64(value);
        break;
      case "f32":
        encoder.pushF32(value);
        break;
      case "f64":
        encoder.pushF64(value);
        break;
    }
  }

  decode(decoder: DataDecoder): number {
    switch (this.size) {
      case "u8":
        return decoder.takeU8();
      case "u16":
        return decoder.takeU16();
      case "u32":
        return decoder.takeU32();
      case "u64":
        return decoder.takeU64();
      case "u128":
        return decoder.takeU128();
      case "i8":
        return decoder.takeI8();
      case "i16":
        return decoder.takeI16();
      case "i32":
        return decoder.takeI32();
      case "i64":
        return decoder.takeI64();
      case "i128":
        return decoder.takeI128();
      case "usize":
        // usize decodes as u64
        return decoder.takeU64();
      case "isize":
        // isize decodes as i64
        return decoder.takeI64();
      case "f32":
        return decoder.takeF32();
      case "f64":
        return decoder.takeF64();
    }
  }
}

class OptionType implements TypeClass {
  private wrappedType: TypeClass;

  constructor(wrappedType: TypeClass) {
    this.wrappedType = wrappedType;
  }

  encode(encoder: DataEncoder, value: any): void {
    if (value === null || value === undefined) {
      encoder.pushU8(0); // Indicate null
    } else {
      encoder.pushU8(1); // Indicate non-null
      this.wrappedType.encode(encoder, value);
    }
  }

  decode(decoder: DataDecoder): any {
    const isPresent = decoder.takeU8();
    if (isPresent === 0) {
      return null; // Return null
    } else {
      return this.wrappedType.decode(decoder);
    }
  }
}

type Ok = { value: any };
type Err = { error: any };

class ResultType implements TypeClass {
  private okType: TypeClass;
  private errType: TypeClass;

  constructor(okType: TypeClass, errType: TypeClass) {
    this.okType = okType;
    this.errType = errType;
  }

  encode(encoder: DataEncoder, value: any): void {
    const result: Ok | Err = value;
    if ("ok" in result) {
      encoder.pushU8(1); // Indicate Ok
      this.okType.encode(encoder, result.ok);
    } else if ("err" in result) {
      encoder.pushU8(0); // Indicate Err
      this.errType.encode(encoder, result.err);
    } else {
      throw new Error("Invalid RustType value: must be Ok or Err");
    }
  }

  decode(decoder: DataDecoder): any {
    const isOk = decoder.takeU8();
    if (isOk === 1) {
      const okValue = this.okType.decode(decoder);
      return { ok: okValue };
    } else {
      const errValue = this.errType.decode(decoder);
      return { err: errValue };
    }
  }
}

/**
 * Type class for array/Vec values with encoding/decoding methods
 */
class ArrayType implements TypeClass {
  private elementType: TypeClass;

  constructor(elementType: TypeClass) {
    this.elementType = elementType;
  }

  encode(encoder: DataEncoder, value: any[]): void {
    encoder.pushU32(value.length);
    for (const element of value) {
      this.elementType.encode(encoder, element);
    }
  }

  decode(decoder: DataDecoder): any[] {
    const length = decoder.takeU32();
    const result: any[] = [];
    for (let i = 0; i < length; i++) {
      result.push(this.elementType.decode(decoder));
    }
    return result;
  }
}

/**
 * Type class for clamped u8 array values (Uint8ClampedArray).
 * Used for canvas ImageData and similar APIs.
 */
class U8ClampedType implements TypeClass {
  encode(encoder: DataEncoder, value: Uint8ClampedArray | number[]): void {
    encoder.pushU32(value.length);
    for (let i = 0; i < value.length; i++) {
      encoder.pushU8(value[i]);
    }
  }

  decode(decoder: DataDecoder): Uint8ClampedArray {
    const length = decoder.takeU32();
    const result = new Uint8ClampedArray(length);
    for (let i = 0; i < length; i++) {
      result[i] = decoder.takeU8();
    }
    return result;
  }
}

const u8ClampedTypeInstance = new U8ClampedType();

// Pre-instantiated numeric type classes
export const U8Type = new NumericType("u8");
export const U16Type = new NumericType("u16");
export const U32Type = new NumericType("u32");
export const U64Type = new NumericType("u64");
export const U128Type = new NumericType("u128");
export const I8Type = new NumericType("i8");
export const I16Type = new NumericType("i16");
export const I32Type = new NumericType("i32");
export const I64Type = new NumericType("i64");
export const I128Type = new NumericType("i128");
export const UsizeType = new NumericType("usize");
export const IsizeType = new NumericType("isize");
export const F32Type = new NumericType("f32");
export const F64Type = new NumericType("f64");

// Pre-instantiated string type class
export const strType = new StringType();

// Pre-instantiated singleton types
const boolTypeInstance = new BoolType();
const nullTypeInstance = new NullType();
const heapRefTypeInstance = new HeapRefType();
const borrowedRefTypeInstance = new BorrowedRefType();
const stringTypeInstance = new StringType();

/**
 * Parse a TypeDef from a byte array and return a TypeClass.
 * This is a recursive function that handles nested callbacks.
 */
function parseTypeDef(bytes: Uint8Array, offset: { value: number }): TypeClass {
  const tag = bytes[offset.value++];

  switch (tag) {
    case TypeTag.Null:
      return nullTypeInstance;
    case TypeTag.Bool:
      return boolTypeInstance;
    case TypeTag.U8:
      return U8Type;
    case TypeTag.U16:
      return U16Type;
    case TypeTag.U32:
      return U32Type;
    case TypeTag.U64:
      return U64Type;
    case TypeTag.U128:
      return U128Type;
    case TypeTag.I8:
      return I8Type;
    case TypeTag.I16:
      return I16Type;
    case TypeTag.I32:
      return I32Type;
    case TypeTag.I64:
      return I64Type;
    case TypeTag.I128:
      return I128Type;
    case TypeTag.F32:
      return F32Type;
    case TypeTag.F64:
      return F64Type;
    case TypeTag.Usize:
      return UsizeType;
    case TypeTag.Isize:
      return IsizeType;
    case TypeTag.String:
      return stringTypeInstance;
    case TypeTag.HeapRef:
      return heapRefTypeInstance;
    case TypeTag.BorrowedRef:
      return borrowedRefTypeInstance;
    case TypeTag.Callback: {
      const paramCount = bytes[offset.value++];
      const paramTypes: TypeClass[] = [];
      for (let i = 0; i < paramCount; i++) {
        paramTypes.push(parseTypeDef(bytes, offset));
      }
      const returnType = parseTypeDef(bytes, offset);
      return new CallbackType(paramTypes, returnType);
    }
    case TypeTag.Option: {
      const innerType = parseTypeDef(bytes, offset);
      return new OptionType(innerType);
    }
    case TypeTag.Result: {
      const okType = parseTypeDef(bytes, offset);
      const errType = parseTypeDef(bytes, offset);
      return new ResultType(okType, errType);
    }
    case TypeTag.Array: {
      const elementType = parseTypeDef(bytes, offset);
      return new ArrayType(elementType);
    }
    case TypeTag.U8Clamped:
      return u8ClampedTypeInstance;
    case TypeTag.StringEnum: {
      // Read variant count
      const variantCount = bytes[offset.value++];
      const lookupArray: string[] = [];

      // Read each variant string
      for (let i = 0; i < variantCount; i++) {
        // Read string length (u32 little-endian)
        const len =
          bytes[offset.value] |
          (bytes[offset.value + 1] << 8) |
          (bytes[offset.value + 2] << 16) |
          (bytes[offset.value + 3] << 24);
        offset.value += 4;

        // Read string bytes and decode as UTF-8
        const strBytes = bytes.subarray(offset.value, offset.value + len);
        offset.value += len;
        lookupArray.push(new TextDecoder().decode(strBytes));
      }

      return new StringEnumType(lookupArray);
    }
    default:
      throw new Error(`Unknown TypeTag: ${tag}`);
  }
}

export {
  TypeClass,
  TypeTag,
  ArrayType,
  BoolType,
  BorrowedRefType,
  HeapRefType,
  CallbackType,
  NullType,
  NumericType,
  OptionType,
  StringType,
  StringEnumType,
  ResultType,
  U8ClampedType,
  parseTypeDef,
};
