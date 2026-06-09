const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const pass_string_vec = () => {
    assert.deepStrictEqual(
        wasm.consume_string_vec(["hello", "world"]),
        ["hello", "world", "Hello from Rust!"],
    );
    assert.deepStrictEqual(
        wasm.consume_optional_string_vec(["hello", "world"]),
        ["hello", "world", "Hello from Rust!"],
    );
    assert.strictEqual(wasm.consume_optional_string_vec(undefined), undefined);
};

export const pass_invalid_string_vec = () => {
    try {
        wasm.consume_string_vec([42]);
    } catch (e) {
        assert.match(e.message, /array contains a value of the wrong type/)
        assert.match(e.stack, /consume_string_vec/)
    }
};
