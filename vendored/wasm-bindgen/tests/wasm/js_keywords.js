const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_keywords_compile = () => {
  assert.strictEqual(wasm._throw(1), 1);
  assert.strictEqual(wasm._class(1, 2), false);
  assert.strictEqual(wasm.classy(3), 3);
  let obj = new wasm.Class("class");
  assert.strictEqual(wasm.Class.void("string"), "string");
  assert.strictEqual(obj.catch, "class");
  assert.strictEqual(obj.instanceof("Class"), "class is instance of Class");
};

export const test_keyword_1_as_fn_name = (x) => {
  return wasm._throw(x);
};

export const test_keyword_2_as_fn_name = (x, y) => {
  return wasm._class(x, y);
};

export const test_keyword_as_fn_arg = (x) => {
  return wasm.classy(x);
};
