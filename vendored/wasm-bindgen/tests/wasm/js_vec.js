const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });
const wasm = new Proxy({}, { get: (_t, n) => window[n] });

// Test if passing large arrays which cause allocation in Wasm are properly handled.
export const pass_array_with_allocation = () => {
    const values = new Array(10_000).fill(1)
    assert.strictEqual(wasm.test_sum(values), 10_000);
};
