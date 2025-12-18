
import { DataEncoder, DataDecoder } from "./encoding";
import { RustFunction } from "./rust_function";

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
    const id = window.jsHeap.insert(obj);
    encoder.pushU64(id);
  }

  decode(decoder: DataDecoder): unknown {
    const id = decoder.takeU64();
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
    encoder.pushU64(fnId);
  }

  decode(decoder: DataDecoder): (...args: any[]) => any {
    const fnId = decoder.takeU64();
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

/**
 * Type class for numeric values (u8, u16, u32, u64) with encoding/decoding methods
 */
class NumericType implements TypeClass {
  private size: 'u8' | 'u16' | 'u32' | 'u64' | 'f32' | 'f64';

  constructor(size: 'u8' | 'u16' | 'u32' | 'u64' | 'f32' | 'f64') {
    this.size = size;
  }

  encode(encoder: DataEncoder, value: number): void {
    switch (this.size) {
      case 'u8':
        encoder.pushU8(value);
        break;
      case 'u16':
        encoder.pushU16(value);
        break;
      case 'u32':
        encoder.pushU32(value);
        break;
      case 'u64':
        encoder.pushU64(value);
        break;
      case 'f32':
        encoder.pushF32(value);
        break;
      case 'f64':
        encoder.pushF64(value);
        break;
    }
  }

  decode(decoder: DataDecoder): number {
    switch (this.size) {
      case 'u8':
        return decoder.takeU8();
      case 'u16':
        return decoder.takeU16();
      case 'u32':
        return decoder.takeU32();
      case 'u64':
        return decoder.takeU64();
      case 'f32':
        return decoder.takeF32();
      case 'f64':
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

/**
 * Creates a wrapper function that handles encoding/decoding for a JS function
 */
function createWrapperFunction(
  paramTypes: TypeClass[],
  returnType: TypeClass,
  jsFunction: (...args: any[]) => any
): (decoder: DataDecoder, encoder: DataEncoder) => void {
  return (decoder: DataDecoder, encoder: DataEncoder) => {
    // Decode parameters using their respective types
    const params = paramTypes.map(paramType => paramType.decode(decoder));
    
    // Call the original JS function with decoded parameters
    const result = jsFunction(...params);
    
    // Encode the result using the return type
    returnType.encode(encoder, result);
  };
}

// Pre-instantiated numeric type classes
export const U8Type = new NumericType('u8');
export const U16Type = new NumericType('u16');
export const U32Type = new NumericType('u32');
export const U64Type = new NumericType('u64');
export const F32Type = new NumericType('f32');
export const F64Type = new NumericType('f64');

// Pre-instantiated string type class
export const strType = new StringType();

export { TypeClass, BoolType, HeapRefType, CallbackType, NullType, NumericType, OptionType, StringType, createWrapperFunction };