const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const pass_struct_vec = () => {
    const el1 = new wasm.ArrayElement();
    const el2 = new wasm.ArrayElement();
    const ret = wasm.consume_struct_vec([el1, el2]);
    assert.strictEqual(ret.length, 3);

    const ret2 = wasm.consume_optional_struct_vec(ret);
    assert.strictEqual(ret2.length, 4);

    assert.strictEqual(wasm.consume_optional_struct_vec(undefined), undefined);
};

export const pass_invalid_struct_vec = () => {
    try {
        wasm.consume_struct_vec(['not a struct']);
    } catch (e) {
        assert.match(e.message, /array contains a value of the wrong type/)
        assert.match(e.stack, /consume_struct_vec/)
    }
};
