const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const call_throw_one = function() {
  try {
    wasm.throw_one();
  } catch (e) {
    assert.strictEqual(e, 1);
  }
};

export const call_ok = function() {
  wasm.nothrow();
};
