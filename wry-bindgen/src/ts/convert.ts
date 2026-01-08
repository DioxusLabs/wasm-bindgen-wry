export function is_undefined(x: any): boolean {
  return x === undefined;
}
export function is_null(x: any): boolean {
  return x === null;
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
export function as_string(x: any): string | null {
  return typeof x === "string" ? x : null;
}
export function as_f64(x: any): number | null {
  return typeof x === "number" ? x : null;
}
export function debug_string(x: any): string {
  try {
    return x.toString();
  } catch {
    return "[unrepresentable]";
  }
}

// Arithmetic operators
export function js_checked_div(a: any, b: any): any {
  try {
    return a / b;
  } catch (e) {
    return e;
  }
}
export function js_pow(a: any, b: any): any {
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

// Other operators
export function js_in(prop: any, obj: any): boolean {
  return prop in obj;
}

// instanceof check for Error
export function is_error(x: any): boolean {
  return x instanceof Error;
}

// Heap management - clone a value in the JS heap
// Returns the value itself (not the ID) - HeapRefType.encode handles inserting it
export function clone_heap_ref(heapId: number): unknown {
  return window.jsHeap.get(heapId);
}

// Heap management - drop a value from the JS heap
export function drop_heap_ref(heapId: number): void {
  window.jsHeap.remove(heapId);
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
