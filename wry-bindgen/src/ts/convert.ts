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
export function debug_string(x: any): string {
  try {
    return x.toString();
  } catch {
    return "[unrepresentable]";
  }
}
export function str_to_jsvalue(n: string): any {
  return n;
}
export function float_to_jsvalue(n: number): any {
  return n;
}
export function int_to_jsvalue(n: number): any {
  return n;
}
