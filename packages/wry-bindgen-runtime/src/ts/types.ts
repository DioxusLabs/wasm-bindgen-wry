import { DataEncoder, DataDecoder } from "./encoding";
import { RustFunction, RustFunctionPolicy } from "./rust_function";

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
  DynamicUnion = 25,
  Char = 26,
  ThrowingResult = 27,
  NumericEnum = 28,
  RustValue = 29,
  RustBorrow = 30,
  MutArray = 31,
}

/**
 * Base interface for all type classes
 */
interface TypeClass {
  encode(encoder: DataEncoder, value: any): void;
  decode(decoder: DataDecoder): any;
  /**
   * Append a `&mut [T]` write-back payload for `value` to `encoder`, if this
   * type carries one (a `MutArray`, or a container nesting one). Most types have
   * nothing to write back. Called after a Rust-to-JS import returns: the mutated
   * arrays travel back to Rust appended after the return value, in argument
   * order, matching how Rust queued the write-backs while encoding the call.
   */
  appendWriteBack?(encoder: DataEncoder, value: any): void;
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
    // Rust assigns the ID. JS either stores immediately for responses or defers
    // request arguments until Rust sends the exact ID to install.
    encoder.pushHeapRef(obj);
  }

  decode(decoder: DataDecoder): unknown {
    const id = decoder.takeU64();
    if (!window.jsHeap.has(id)) {
      throw new Error(`Unknown JS heap reference ID: ${id}`);
    }
    // A return value transfers ownership to JS (the Rust side forgot it), so
    // take the slot. Everywhere else this is a borrow, so just read it.
    return window.jsHeap.isTakingOwnership()
      ? window.jsHeap.remove(id)
      : window.jsHeap.get(id);
  }
}

/**
 * Type class for an exported Rust struct passed or returned by value.
 *
 * On the wire it is encoded and decoded exactly like a {@link HeapRefType}: the
 * object wrapper rides the JS heap and Rust extracts its handle. The distinct
 * type carries wasm-bindgen's moved-value semantics — a by-value pass transfers
 * ownership to Rust, so the caller (see `callExport`) zeroes the wrapper's
 * `__handle` afterward and a later use throws "Attempt to use a moved value".
 */
class RustValueType implements TypeClass {
  // The exported struct's class name, used to reject a by-value pass of an
  // inheritance descendant as its ancestor.
  constructor(private readonly className: string = "") {}

  encode(encoder: DataEncoder, obj: any): void {
    // A wrapper whose handle was already zeroed (consumed) is being reused.
    if (obj != null && obj.__handle === 0) {
      throw new Error("Attempt to use a moved value");
    }
    // Consuming a descendant by-value as its ancestor would hand the
    // descendant's object to the ancestor's store slot (type confusion). The
    // ancestor slot of a true descendant is a separate handle from its own, so
    // reject. A direct instance (or a JS-only subclass whose super() set the
    // slot equal to the own handle) passes.
    if (obj != null && this.className) {
      const slot = obj["__wbg_ptr_" + this.className];
      if (typeof slot === "number" && slot !== obj.__handle) {
        throw new TypeError(
          `${this.className}: cannot be consumed by-value as its ancestor`,
        );
      }
    }
    encoder.pushHeapRef(obj);
  }

  decode(decoder: DataDecoder): unknown {
    return heapRefTypeInstance.decode(decoder);
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
    if (!window.jsHeap.has(id)) {
      throw new Error(`Unknown borrowed JS reference ID: ${id}`);
    }
    // Works for both heap refs (128+) and borrow stack refs (1-127)
    return window.jsHeap.get(id);
  }
}

/**
 * Type class for a `&T` argument to an exported Rust struct. The routed object
 * handle rides the wire as a plain `u32` (like a method receiver), so Rust reads
 * it directly without a borrow-stack round-trip. An inheritance descendant passed
 * as `&Ancestor` is routed to its shared ancestor-view handle (`__wbg_ptr_<Class>`)
 * so the checked-out object really is the ancestor's shared `T`.
 */
class RustBorrowType implements TypeClass {
  constructor(private readonly className: string) {}

  encode(encoder: DataEncoder, obj: any): void {
    if (obj == null) {
      throw new TypeError(`expected a ${this.className} instance, got null`);
    }
    if (obj.__handle === 0) {
      throw new Error("Attempt to use a moved value");
    }
    const slot = obj["__wbg_ptr_" + this.className];
    const handle = typeof slot === "number" ? slot : obj.__handle;
    encoder.pushU32(handle);
  }

  decode(_decoder: DataDecoder): unknown {
    throw new Error("RustBorrow is an argument-only wire type");
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
 * Rust `char`: the wire payload is the u32 code point, but in JS it is a 1-character
 * string (matching wasm-bindgen).
 */
class CharType implements TypeClass {
  // The argument is named `c` so a non-string input produces the same
  // "c.codePointAt is not a function" TypeError that wasm-bindgen's glue does.
  encode(encoder: DataEncoder, c: string): void {
    encoder.pushU32(c.codePointAt(0) ?? 0);
  }

  decode(decoder: DataDecoder): string {
    return String.fromCodePoint(decoder.takeU32());
  }
}

/**
 * Type class for string enum values with u32 encoding and lookup arrays
 */
class StringEnumType implements TypeClass {
  declare private lookupArray: string[];

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
 * Type class for a C-style enum. Encoding rejects values that are not one of the
 * declared variants (a non-number, or a number outside the variant set), so a
 * wrong-typed value throws here instead of silently coercing to a variant.
 */
class NumericEnumType implements TypeClass {
  declare private signed: boolean;
  declare private values: Set<number>;

  constructor(signed: boolean, values: number[]) {
    this.signed = signed;
    this.values = new Set(values);
  }

  encode(encoder: DataEncoder, value: number): void {
    if (typeof value !== "number" || !this.values.has(value)) {
      throw new Error("the value provided is not a valid enum value");
    }
    encoder.pushU32(value >>> 0);
  }

  decode(decoder: DataDecoder): number {
    return this.signed ? decoder.takeI32() : decoder.takeU32();
  }
}

type DynamicUnionVariant =
  | { kind: "string"; value: string }
  | { kind: "type"; type: TypeClass };

/**
 * Type class for dynamic unions. JS-to-Rust values are sent as heap refs so
 * Rust can run the same runtime dispatch as upstream; Rust-to-JS values are
 * decoded from a variant index plus the variant payload.
 */
class DynamicUnionType implements TypeClass {
  declare private variants: DynamicUnionVariant[];

  constructor(variants: DynamicUnionVariant[]) {
    this.variants = variants;
  }

  encode(encoder: DataEncoder, value: unknown): void {
    heapRefTypeInstance.encode(encoder, value);
  }

  decode(decoder: DataDecoder): unknown {
    const index = decoder.takeU8();
    const variant = this.variants[index];
    if (variant === undefined) {
      throw new Error(`Invalid dynamic union variant index: ${index}`);
    }
    if (variant.kind === "string") {
      return variant.value;
    }
    return variant.type.decode(decoder);
  }
}

/**
 * Type class for Rust callbacks with encoding/decoding methods
 */
class CallbackType implements TypeClass {
  declare private paramTypes: TypeClass[];
  declare private returnType: TypeClass;

  constructor(paramTypes: TypeClass[], returnType: TypeClass) {
    this.paramTypes = paramTypes;
    this.returnType = returnType;
  }

  encode(encoder: DataEncoder, fnId: number): void {
    encoder.pushU32(fnId);
  }

  decode(decoder: DataDecoder): (...args: any[]) => any {
    const fnId = decoder.takeU32();
    const policy = decoder.takeU32() as RustFunctionPolicy;
    const rustFunction = new RustFunction(fnId, this.paramTypes, this.returnType, policy);
    const callable = (...args: any[]) => rustFunction.call(...args);
    Object.defineProperty(callable, "__wryRustFunction", {
      value: rustFunction,
    });
    return callable;
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
  declare private size: NumberType;

  constructor(size: NumberType) {
    this.size = size;
  }

  encode(encoder: DataEncoder, value: number | bigint): void {
    switch (this.size) {
      case "u8":
        encoder.pushU8(Number(value));
        break;
      case "u16":
        encoder.pushU16(Number(value));
        break;
      case "u32":
        encoder.pushU32(Number(value));
        break;
      case "u64":
        // i64/u64 are BigInt; pushU64 also accepts a plain number.
        encoder.pushU64(value);
        break;
      case "u128":
        encoder.pushU128(value);
        break;
      case "i8":
        // Signed integers encode as unsigned (Rust: self as u8)
        encoder.pushU8(Number(value) & 0xff);
        break;
      case "i16":
        // Signed integers encode as unsigned (Rust: self as u16)
        encoder.pushU16(Number(value) & 0xffff);
        break;
      case "i32":
        // Signed integers encode as unsigned (Rust: self as u32)
        encoder.pushU32(Number(value) >>> 0);
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
        // usize encodes as u64 but stays a JS number (wasm32 pointer width).
        encoder.pushU64(Number(value));
        break;
      case "isize":
        // isize encodes as u64 (Rust: self as u64) but stays a JS number.
        encoder.pushU64(Number(value));
        break;
      case "f32":
        encoder.pushF32(Number(value));
        break;
      case "f64":
        encoder.pushF64(Number(value));
        break;
    }
  }

  decode(decoder: DataDecoder): number | bigint {
    switch (this.size) {
      case "u8":
        return decoder.takeU8();
      case "u16":
        return decoder.takeU16();
      case "u32":
        return decoder.takeU32();
      case "u64":
        return decoder.takeBigUint64();
      case "u128":
        return decoder.takeBigUint128();
      case "i8":
        return decoder.takeI8();
      case "i16":
        return decoder.takeI16();
      case "i32":
        return decoder.takeI32();
      case "i64":
        return decoder.takeBigInt64();
      case "i128":
        return decoder.takeBigInt128();
      case "usize":
        // usize decodes as a plain number (wasm32 pointer width).
        return decoder.takeU64();
      case "isize":
        // isize decodes as a plain number (wasm32 pointer width).
        return decoder.takeI64();
      case "f32":
        return decoder.takeF32();
      case "f64":
        return decoder.takeF64();
    }
  }
}

class OptionType implements TypeClass {
  declare private wrappedType: TypeClass;

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
      return undefined; // `None` decodes to `undefined`, matching wasm-bindgen
    } else {
      return this.wrappedType.decode(decoder);
    }
  }

  appendWriteBack(encoder: DataEncoder, value: any): void {
    // A `None` registers no write-back on the Rust side, so append one only for
    // a present value (e.g. `Some(&mut [T])`), matching the Rust ordering.
    if (value !== null && value !== undefined) {
      this.wrappedType.appendWriteBack?.(encoder, value);
    }
  }
}

type Ok = { value: any };
type Err = { error: any };

class ResultType implements TypeClass {
  declare private okType: TypeClass;
  declare private errType: TypeClass;

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
 * A `Result` returned from an exported Rust function. The wire payload matches
 * `Result`, but the `Err` value is thrown as an exception instead of being
 * returned as a `{ err }` object — matching wasm-bindgen's behavior for fallible
 * exports.
 */
class ThrowingResultType implements TypeClass {
  declare private okType: TypeClass;
  declare private errType: TypeClass;

  constructor(okType: TypeClass, errType: TypeClass) {
    this.okType = okType;
    this.errType = errType;
  }

  encode(encoder: DataEncoder, value: any): void {
    const result: Ok | Err = value;
    if ("ok" in result) {
      encoder.pushU8(1);
      this.okType.encode(encoder, result.ok);
    } else if ("err" in result) {
      encoder.pushU8(0);
      this.errType.encode(encoder, result.err);
    } else {
      throw new Error("Invalid RustType value: must be Ok or Err");
    }
  }

  decode(decoder: DataDecoder): any {
    const isOk = decoder.takeU8();
    if (isOk === 1) {
      return this.okType.decode(decoder);
    }
    throw this.errType.decode(decoder);
  }
}

/**
 * Type class for array/Vec values with encoding/decoding methods
 */
class ArrayType implements TypeClass {
  declare private elementType: TypeClass;

  constructor(elementType: TypeClass) {
    this.elementType = elementType;
  }

  encode(encoder: DataEncoder, value: any[]): void {
    encoder.pushU32(value.length);
    for (const element of value) {
      try {
        this.elementType.encode(encoder, element);
      } catch {
        // An element of the wrong type surfaces as wasm-bindgen's array message.
        throw new Error("array contains a value of the wrong type");
      }
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
 * Type class for a mutable array argument (`&mut [T]`).
 *
 * On the wire it is identical to its inner array type, but the distinct class
 * lets the caller copy the mutated elements back into the original JS array
 * after the call returns — wry has no shared linear memory, so the receiver
 * appends the (possibly mutated) array to its response and the caller copies it
 * back. `encode`/`decode` delegate to the inner array type; `copyBack` reads a
 * write-back payload and writes it element-by-element into `target`.
 */
class MutArrayType implements TypeClass {
  declare readonly inner: TypeClass;

  constructor(inner: TypeClass) {
    this.inner = inner;
  }

  encode(encoder: DataEncoder, value: any): void {
    this.inner.encode(encoder, value);
  }

  decode(decoder: DataDecoder): any {
    return this.inner.decode(decoder);
  }

  /**
   * Decode a write-back payload and copy its elements into `target` (the
   * caller's original array), keeping `target`'s identity and length.
   */
  copyBack(decoder: DataDecoder, target: any): void {
    const updated = this.inner.decode(decoder);
    const count = Math.min(target.length, updated.length);
    for (let i = 0; i < count; i++) {
      target[i] = updated[i];
    }
  }

  appendWriteBack(encoder: DataEncoder, value: any): void {
    this.inner.encode(encoder, value);
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
const U8Type = new NumericType("u8");
const U16Type = new NumericType("u16");
const U32Type = new NumericType("u32");
const U64Type = new NumericType("u64");
const U128Type = new NumericType("u128");
const I8Type = new NumericType("i8");
const I16Type = new NumericType("i16");
const I32Type = new NumericType("i32");
const I64Type = new NumericType("i64");
const I128Type = new NumericType("i128");
const UsizeType = new NumericType("usize");
const IsizeType = new NumericType("isize");
const F32Type = new NumericType("f32");
const F64Type = new NumericType("f64");

// Pre-instantiated singleton types
const boolTypeInstance = new BoolType();
const nullTypeInstance = new NullType();
const heapRefTypeInstance = new HeapRefType();
const borrowedRefTypeInstance = new BorrowedRefType();
const stringTypeInstance = new StringType();
const charTypeInstance = new CharType();

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
    case TypeTag.Char:
      return charTypeInstance;
    case TypeTag.HeapRef:
      return heapRefTypeInstance;
    case TypeTag.RustValue: {
      const len =
        bytes[offset.value] |
        (bytes[offset.value + 1] << 8) |
        (bytes[offset.value + 2] << 16) |
        (bytes[offset.value + 3] << 24);
      offset.value += 4;
      const strBytes = bytes.subarray(offset.value, offset.value + len);
      offset.value += len;
      return new RustValueType(new TextDecoder().decode(strBytes));
    }
    case TypeTag.BorrowedRef:
      return borrowedRefTypeInstance;
    case TypeTag.RustBorrow: {
      const len =
        bytes[offset.value] |
        (bytes[offset.value + 1] << 8) |
        (bytes[offset.value + 2] << 16) |
        (bytes[offset.value + 3] << 24);
      offset.value += 4;
      const strBytes = bytes.subarray(offset.value, offset.value + len);
      offset.value += len;
      return new RustBorrowType(new TextDecoder().decode(strBytes));
    }
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
    case TypeTag.ThrowingResult: {
      const okType = parseTypeDef(bytes, offset);
      const errType = parseTypeDef(bytes, offset);
      return new ThrowingResultType(okType, errType);
    }
    case TypeTag.Array: {
      const elementType = parseTypeDef(bytes, offset);
      return new ArrayType(elementType);
    }
    case TypeTag.MutArray: {
      const innerType = parseTypeDef(bytes, offset);
      return new MutArrayType(innerType);
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
    case TypeTag.NumericEnum: {
      const signed = bytes[offset.value++] !== 0;
      const variantCount = bytes[offset.value++];
      const values: number[] = [];
      for (let i = 0; i < variantCount; i++) {
        const raw =
          (bytes[offset.value] |
            (bytes[offset.value + 1] << 8) |
            (bytes[offset.value + 2] << 16) |
            (bytes[offset.value + 3] << 24)) >>>
          0;
        offset.value += 4;
        values.push(signed ? raw | 0 : raw);
      }
      return new NumericEnumType(signed, values);
    }
    case TypeTag.DynamicUnion: {
      const variantCount = bytes[offset.value++];
      const variants: DynamicUnionVariant[] = [];

      for (let i = 0; i < variantCount; i++) {
        const kind = bytes[offset.value++];
        if (kind === 0) {
          const len =
            bytes[offset.value] |
            (bytes[offset.value + 1] << 8) |
            (bytes[offset.value + 2] << 16) |
            (bytes[offset.value + 3] << 24);
          offset.value += 4;

          const strBytes = bytes.subarray(offset.value, offset.value + len);
          offset.value += len;
          variants.push({
            kind: "string",
            value: new TextDecoder().decode(strBytes),
          });
        } else if (kind === 1) {
          variants.push({
            kind: "type",
            type: parseTypeDef(bytes, offset),
          });
        } else {
          throw new Error(`Invalid dynamic union variant kind: ${kind}`);
        }
      }

      return new DynamicUnionType(variants);
    }
    default:
      throw new Error(`Unknown TypeTag: ${tag}`);
  }
}

export {
  TypeClass,
  HeapRefType,
  RustValueType,
  MutArrayType,
  parseTypeDef,
};
