const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_c_style_enum = () => {
    assert.strictEqual(wasm.Color.Green, 0);
    assert.strictEqual(wasm.Color.Yellow, 1);
    assert.strictEqual(wasm.Color.Red, 2);
    assert.strictEqual(wasm.Color[0], 'Green');
    assert.strictEqual(wasm.Color[1], 'Yellow');
    assert.strictEqual(wasm.Color[2], 'Red');
    assert.strictEqual(Object.keys(wasm.Color).length, 6);

    assert.strictEqual(wasm.enum_cycle(wasm.Color.Green), wasm.Color.Yellow);
};

export const js_c_style_enum_with_custom_values = () => {
    assert.strictEqual(wasm.ColorWithCustomValues.Green, 21);
    assert.strictEqual(wasm.ColorWithCustomValues.Yellow, 34);
    assert.strictEqual(wasm.ColorWithCustomValues.Red, 2);
    assert.strictEqual(wasm.ColorWithCustomValues[21], 'Green');
    assert.strictEqual(wasm.ColorWithCustomValues[34], 'Yellow');
    assert.strictEqual(wasm.ColorWithCustomValues[2], 'Red');
    assert.strictEqual(Object.keys(wasm.ColorWithCustomValues).length, 6);

    assert.strictEqual(wasm.enum_with_custom_values_cycle(wasm.ColorWithCustomValues.Green), wasm.ColorWithCustomValues.Yellow);
};

export const js_handle_optional_enums = x => wasm.handle_optional_enums(x);

export const js_expect_enum = (a, b) => {
  assert.strictEqual(a, b);
};

export const js_expect_enum_none = a => {
  assert.strictEqual(a, undefined);
};

export const js_renamed_enum = b => {
  assert.strictEqual(wasm.JsRenamedEnum.B, b);
};

export const js_enum_with_error_variant = () => {
    assert.strictEqual(wasm.EnumWithErrorVariant.Error, 2);
};

// Helper to create a Foo object for testing
export const makeFoo = () => {
    return { type: 'Foo', data: 'test' };
};

// Round-trip helpers that force the wasm/JS boundary so the dynamic-union
// dispatcher actually runs. Each just calls back into the corresponding
// exported Rust function with the value unchanged.
export const js_string_enum_fallback_roundtrip = e => wasm.string_enum_fallback_roundtrip(e);
export const js_nested_union_roundtrip = o => wasm.nested_union_roundtrip(o);
export const js_optional_union_roundtrip = o => wasm.optional_union_roundtrip(o);
export const js_fallback_union_roundtrip = u => wasm.fallback_union_roundtrip(u);

// Async round-trip: returning from an `async function` produces a
// `Promise<Union>` on the JS side; awaiting it on the Rust import side
// requires `From<Promise<Union>> for JsFuture<Union>` to compile.
export const js_async_union_roundtrip = async o => wasm.async_union_roundtrip(o);

// Same shape, `Result<Union, JsValue>` form (success-only here; reject
// path is exercised by other catch tests in the suite).
export const js_async_union_result = async o => wasm.async_union_roundtrip(o);
