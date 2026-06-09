const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });
const wasm = new Proxy({}, { get: (_t, n) => window[n] });

const pointerIndex = (ptr, stride) => ptr / stride;

const nonNullZero = () => 0;

const nonNullTypeError = () =>
  /expected a number argument that is not 0/;

export const test_add = function() {
  assert.strictEqual(wasm.simple_add(1, 2), 3);
  assert.strictEqual(wasm.simple_add(2, 3), 5);
  assert.strictEqual(wasm.simple_add3(2), 5);
  assert.strictEqual(wasm.simple_get2(true), 2);
  assert.strictEqual(wasm.simple_return_and_take_bool(true, false), false);
};

export const test_string_arguments = function() {
  wasm.simple_assert_foo("foo");
  wasm.simple_assert_foo_and_bar("foo2", "bar");
};

export const test_return_a_string = function() {
  assert.strictEqual(wasm.simple_clone("foo"), "foo");
  assert.strictEqual(wasm.simple_clone("another"), "another");
  assert.strictEqual(wasm.simple_concat("a", "b", 3), "a b 3");
  assert.strictEqual(wasm.simple_concat("c", "d", -2), "c d -2");
};

export const test_wrong_types = function() {
  // Skipped on wry: the upstream test is gated on `require('process').env`, a
  // nodejs-only API, and only runs under wasm-bindgen's `--debug` argument
  // type checks. wry validates argument types at decode time independently.
};

export const test_other_exports_still_available = function() {
  // Skipped on wry: this reaches into `__wasm` (the raw wasm instance's
  // exports) to call a plain `#[no_mangle] extern "C"` symbol, a wasm-module
  // intrinsic. On the native target `foo` is an ordinary Rust symbol with no
  // JS binding, so there is no instance-exports table to read it from.
};

export const test_jsvalue_typeof = function() {
  assert.ok(wasm.is_object({}));
  assert.ok(!wasm.is_object(42));
  assert.ok(wasm.is_function(function() {}));
  assert.ok(!wasm.is_function(42));
  assert.ok(wasm.is_string("2b or !2b"));
  assert.ok(!wasm.is_string(42));
};

export const optional_str_none = function(x) {
  assert.strictEqual(x, undefined);
};

export const optional_str_some = function(x) {
  assert.strictEqual(x, 'x');
};

export const optional_slice_none = function(x) {
  assert.strictEqual(x, undefined);
};

export const optional_slice_some = function(x) {
  assert.strictEqual(x.length, 3);
  assert.strictEqual(x[0], 1);
  assert.strictEqual(x[1], 2);
  assert.strictEqual(x[2], 3);
}

export const optional_string_none = function(x) {
  assert.strictEqual(x, undefined);
};

export const optional_string_some = function(x) {
  assert.strictEqual(x, 'abcd');
};

export const optional_string_some_empty = function(x) {
  assert.strictEqual(x, '');
};

export const return_string_none = function() {};
export const return_string_some = function() {
  return 'foo';
};

export const test_rust_optional = function() {
  wasm.take_optional_str_none();
  wasm.take_optional_str_none(null);
  wasm.take_optional_str_none(undefined);
  wasm.take_optional_str_some('hello');
  assert.strictEqual(wasm.return_optional_str_none(), undefined);
  assert.strictEqual(wasm.return_optional_str_some(), 'world');
};

export const RenamedInRust = class {};
export const new_renamed = () => new RenamedInRust;

export const import_export_same_name = () => {};

export const test_string_roundtrip = () => {
  const test = s => {
    assert.strictEqual(wasm.do_string_roundtrip(s), s);
  };

  test('');
  test('a');
  test('💖');

  test('a longer string');
  test('a longer 💖 string');
};

export const test_raw_pointers = function() {
  // Skipped on wry: this reads `wasm.__wasm.memory.buffer` as a typed array to
  // inspect Rust heap allocations through wasm linear memory, a wasm intrinsic
  // that has no analogue on the native target.
};

export const test_non_null = function() {
  // Skipped on wry: this round-trips raw `NonNull` addresses that are validated
  // against `wasm.__wasm.memory.buffer`, a wasm linear memory intrinsic with no
  // analogue on the native target.
};
