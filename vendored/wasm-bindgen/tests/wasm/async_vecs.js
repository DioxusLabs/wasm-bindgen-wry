const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_works = async () => {
    assert.deepStrictEqual(await wasm.async_jsvalue_vec(), [1, "hi", new Float64Array(), null]);
    assert.deepStrictEqual(await wasm.async_import_vec(), [/hi|bye/, /hello w[a-z]rld/]);
    assert.deepStrictEqual(await wasm.async_string_vec(), ["a", "b", "c"]);
    assert.strictEqual((await wasm.async_struct_vec()).length, 2);
    assert.deepStrictEqual(await wasm.async_enum_vec(), [wasm.AnotherEnum.C, wasm.AnotherEnum.A, wasm.AnotherEnum.B]);

    const numberVec = await wasm.async_number_vec();
    assert.deepStrictEqual(numberVec, [1, -3, 7, 12]);
    // wry returns a numeric `Vec<T>` as a plain `Array`, not a typed array
    // backed by linear memory, so the `Int32Array`/`.buffer` identity check
    // (that it is a fresh, GC-able view) does not apply here.
    // assert.strictEqual(numberVec.byteLength, numberVec.buffer.byteLength);
};
