const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_identity = a => a;

export const js_works = () => {
    assert.strictEqual(wasm.letter(), 'a');
    assert.strictEqual(wasm.face(), '😀');
    assert.strictEqual(wasm.rust_identity(''), '\u0000');
    assert.strictEqual(wasm.rust_identity('Ղ'), 'Ղ');
    assert.strictEqual(wasm.rust_identity('ҝ'), 'ҝ');
    assert.strictEqual(wasm.rust_identity('Δ'), 'Δ');
    assert.strictEqual(wasm.rust_identity('䉨'), '䉨');
    assert.strictEqual(wasm.rust_js_identity('a'), 'a');
    assert.strictEqual(wasm.rust_js_identity('㊻'), '㊻');
    wasm.rust_letter('a');
    wasm.rust_face('😀');

    assert.strictEqual(wasm.rust_option_identity(undefined), undefined);
    assert.strictEqual(wasm.rust_option_identity(null), undefined);
    assert.strictEqual(wasm.rust_option_identity(''), '\u0000');
    assert.strictEqual(wasm.rust_option_identity('\u0000'), '\u0000');

    assert.throws(() => wasm.rust_identity(55357), /c.codePointAt is not a function/);
    assert.throws(() => wasm.rust_identity('\uD83D'), /expected a valid Unicode scalar value, found 55357/);
    assert.throws(() => wasm.rust_option_identity('\uD83D'), /expected a valid Unicode scalar value, found 55357/);
};
