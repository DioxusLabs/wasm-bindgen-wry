export function is_undefined(x: any): boolean {
  return x === undefined;
}
export function is_null(x: any): boolean {
  return x === null;
}
export function is_null_or_undefined(x: any): boolean {
  return x === null || x === undefined;
}
export function is_array(x: any): boolean {
  return Array.isArray(x);
}
export function is_true(x: any): boolean {
  return x === true;
}
export function is_false(x: any): boolean {
  return x === false;
}
export function get_typeof(x: any): string {
  return typeof x;
}
export function is_falsy(x: any): boolean {
  return !x;
}
export function is_truthy(x: any): boolean {
  return !!x;
}
export function is_object(x: any): boolean {
  return typeof x === "object" && x !== null;
}
export function is_function(x: any): boolean {
  return typeof x === "function";
}
export function is_string(x: any): boolean {
  return typeof x === "string";
}
export function is_symbol(x: any): boolean {
  return typeof x === "symbol";
}
export function is_bigint(x: any): boolean {
  return typeof x === "bigint";
}
export function bigint_from_str(x: string): bigint {
  return BigInt(x);
}
export function symbol_new(description: string | null): symbol {
  return Symbol(description ?? undefined);
}
export function bigint_get_as_i64(x: any): bigint | null {
  // The low 64 bits as a signed value, or null if not a bigint. Callers
  // reinterpret the bits and round-trip-check the value, matching
  // wasm-bindgen's `__wbindgen_bigint_get_as_i64`.
  return typeof x === "bigint" ? BigInt.asIntN(64, x) : null;
}
export function reflect_get(target: any, key: any): any {
  return Reflect.get(target, key);
}
export function as_string(x: any): string | null {
  return typeof x === "string" ? x : null;
}
export function as_f64(x: any): number | null {
  return typeof x === "number" ? x : null;
}
// Unary `+` coercion (wasm-bindgen `__wbindgen_as_number`). May throw on e.g. a Symbol.
export function as_number(x: any): number {
  return +x;
}
// Unary `+` coercion that returns the coerced number, or the thrown error value, so the
// caller can distinguish success (a number) from failure (wasm-bindgen `__wbindgen_try_into_number`).
export function try_into_number(x: any): any {
  try {
    return +x;
  } catch (e) {
    return e;
  }
}
// Mirrors wasm-bindgen's `debugString` intrinsic so `Debug for JsValue` matches.
export function debug_string(val: any): string {
  const type = typeof val;
  if (type == "number" || type == "boolean" || val == null) {
    return `${val}`;
  }
  if (type == "string") {
    return `"${val}"`;
  }
  if (type == "symbol") {
    const description = val.description;
    return description == null ? "Symbol" : `Symbol(${description})`;
  }
  if (type == "function") {
    const name = val.name;
    return typeof name == "string" && name.length > 0 ? `Function(${name})` : "Function";
  }
  if (Array.isArray(val)) {
    const length = val.length;
    let debug = "[";
    if (length > 0) {
      debug += debug_string(val[0]);
    }
    for (let i = 1; i < length; i++) {
      debug += ", " + debug_string(val[i]);
    }
    debug += "]";
    return debug;
  }
  const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
  let className;
  if (builtInMatches && builtInMatches.length > 1) {
    className = builtInMatches[1];
  } else {
    return toString.call(val);
  }
  if (className == "Object") {
    try {
      return "Object(" + JSON.stringify(val) + ")";
    } catch (_) {
      return "Object";
    }
  }
  if (val instanceof Error) {
    return `${val.name}: ${val.message}\n${val.stack}`;
  }
  return className;
}

// Arithmetic operators
export function js_checked_div(a: any, b: any): any {
  // Keep in sync with wasm-bindgen's `Intrinsic::CheckedDiv` JS emission.
  try {
    return a / b;
  } catch (e) {
    if (e instanceof RangeError) {
      return e;
    }
    throw e;
  }
}
export function js_pow(a: any, b: any): any {
  // Keep in sync with wasm-bindgen's `Intrinsic::Pow` JS emission.
  return a ** b;
}
export function js_add(a: any, b: any): any {
  return a + b;
}
export function js_sub(a: any, b: any): any {
  return a - b;
}
export function js_mul(a: any, b: any): any {
  return a * b;
}
export function js_div(a: any, b: any): any {
  return a / b;
}
export function js_rem(a: any, b: any): any {
  return a % b;
}
export function js_neg(a: any): any {
  return -a;
}

// Bitwise operators
export function js_bit_and(a: any, b: any): any {
  return a & b;
}
export function js_bit_or(a: any, b: any): any {
  return a | b;
}
export function js_bit_xor(a: any, b: any): any {
  return a ^ b;
}
export function js_bit_not(a: any): any {
  return ~a;
}
export function js_shl(a: any, b: any): any {
  return a << b;
}
export function js_shr(a: any, b: any): any {
  return a >> b;
}
export function js_unsigned_shr(a: any, b: any): number {
  return a >>> b;
}

// Comparison operators
export function js_lt(a: any, b: any): boolean {
  return a < b;
}
export function js_le(a: any, b: any): boolean {
  return a <= b;
}
export function js_gt(a: any, b: any): boolean {
  return a > b;
}
export function js_ge(a: any, b: any): boolean {
  return a >= b;
}
export function js_loose_eq(a: any, b: any): boolean {
  return a == b;
}
// Strict `===`, matching wasm-bindgen's `PartialEq for JsValue`.
export function js_strict_eq(a: any, b: any): boolean {
  return a === b;
}

// Other operators
export function js_in(prop: any, obj: any): boolean {
  return prop in obj;
}

// instanceof check for Error
export function is_error(x: any): boolean {
  return x instanceof Error;
}

// Heap management - clone a value in the JS heap
// Returns the value itself. HeapRefType.encode handles inserting it and
// encoding the assigned ID when this is returned to Rust.
export function clone_heap_ref(value: unknown): unknown {
  return value;
}

// Heap management - drop a value from the JS heap. The id crosses as a `u64`,
// so it arrives as a BigInt; the heap Map is keyed by Number. Heap ids are
// small slab indices, so the conversion is lossless.
export function drop_heap_ref(heapId: number | bigint): void {
  window.jsHeap.remove(Number(heapId));
}

// Create a wrapper object for an exported Rust struct
export function create_rust_object_wrapper(handle: number, className: string): unknown {
  return window.rustExports.createWrapper(handle, className);
}

// Extract the Rust object handle from a JavaScript wrapper object
// Returns the handle if present, -1 otherwise
export function extract_rust_handle(obj: any): number | null {
  return (obj && typeof obj.__handle === 'number') ? obj.__handle : null;
}
