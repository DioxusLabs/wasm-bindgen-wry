const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_works = () => {
    let called = false;
    wasm.run({
        bar() {
            called = true;
        },
        baz: 1,
    });
    assert.strictEqual(called, true);
};
