const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });
const wasm = new Proxy({}, { get: (_t, n) => window[n] });

let ARG = null;
let ANOTHER_ARG = null;
let SYM = Symbol('a');

export const simple_foo = function(s) {
  assert.strictEqual(ARG, null);
  assert.strictEqual(s, "foo");
  ARG = s;
};

export const simple_another = function(s) {
  assert.strictEqual(ANOTHER_ARG, null);
  assert.strictEqual(s, 21);
  ANOTHER_ARG = s;
  return 35;
};

export const simple_take_and_return_bool = function(s) {
  return s;
};
export const simple_return_object = function() {
  return SYM;
};
export const test_simple = function() {
  assert.strictEqual(ARG, null);
  wasm.simple_take_str("foo");
  assert.strictEqual(ARG, "foo");

  assert.strictEqual(ANOTHER_ARG, null);
  assert.strictEqual(wasm.simple_another_thunk(21), 35);
  assert.strictEqual(ANOTHER_ARG, 21);

  assert.strictEqual(wasm.simple_bool_thunk(true), true);
  assert.strictEqual(wasm.simple_bool_thunk(false), false);

  assert.strictEqual(wasm.simple_get_the_object(), SYM);
};

export const return_string = function() {
  return 'bar';
};

export const take_and_ret_string = function(a) {
  return a + 'b';
};

export const exceptions_throw = function() {
  throw new Error('error!');
};
export const exceptions_throw2 = function() {
  throw new Error('error2');
};
export const test_exception_propagates = function() {
  assert.throws(wasm.exceptions_propagate, /error!/);
};

export const assert_valid_error = function(obj) {
  assert.strictEqual(obj instanceof Error, true);
  assert.strictEqual(obj.message, 'error2');
};

export const IMPORT = 1.0;

export const return_three = function() { return 3; };

export const underscore = function(x) {};

export const pub = function() { return 2; };

export const bar = { foo: 3 };

let CUSTOM_TYPE = null;

export const take_custom_type = function(f) {
  CUSTOM_TYPE = f;
  return f;
};

export const custom_type_return_2 = function() {
  return 2;
};

export const touch_custom_type = function() {
  assert.throws(() => CUSTOM_TYPE.touch(),
    /Attempt to use a moved value|null pointer passed to rust/);
};

export const interpret_2_as_custom_type = function() {
  assert.throws(wasm.interpret_2_as_custom_type, /expected instance of CustomType/);
};

export const baz$ = function() {};
export const $foo = 1.0;

export const assert_dead_import_not_generated = function() {
  // Skipped sub-case: upstream reads the generated bindings file off disk via
  // `require.resolve("wasm-bindgen-test")` + `fs.readFileSync` to assert the
  // `unused_import` symbol was tree-shaken out of the emitted bindings. Both
  // `require.resolve` and the `fs` module are nodejs-only build/codegen-artifact
  // inspection APIs with no analogue in the wry runtime; this asserts a
  // build-time property, not runtime behavior.
};

export const import_inside_function_works = function() {};
export const import_inside_private_module = function() {};
export const should_call_undefined_functions = () => false;

export const STATIC_STRING = 'x';

class StaticMethodCheck {
  static static_method_of_right_this() {
    assert.ok(this === StaticMethodCheck);
  }
}

export { StaticMethodCheck };

export const receive_undefined = val => {
  assert.strictEqual(val, undefined);
};

const VAL = {};

export const receive_some = val => {
  assert.strictEqual(val, VAL);
};

export const get_some_val = () => VAL;

export const Math = {
  func_from_module_math: (a) => a * 2
}

export const Number = {
  func_from_module_number: () => 3.0
}

export const same_name_from_import = (a) => a * 3;

export const same_js_namespace_from_module = {
  func_from_module_1_same_js_namespace: (a) => a * 5
}

const kebab_case = () => 42;
const string_literal_breakers = () => 42;
export { kebab_case as "kebab-case", string_literal_breakers as "\"string'literal\nbreakers\r" };
