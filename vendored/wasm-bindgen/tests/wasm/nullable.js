const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const return_null = () => null;

export const return_undefined = () => undefined;

export const return_number = () => 42;

export const return_string = () => "hello";

export const take_nullable_null = (val) => {
    assert.strictEqual(val, undefined, `expected undefined, got ${val}`);
};

export const take_nullable_value = (val) => {
    assert.ok(val !== null && val !== undefined,
        `expected a value, got ${val}`);
    assert.strictEqual(val, 123);
};

export const take_nullable_number = (val) => {
    assert.ok(val !== null && val !== undefined,
        `expected a number, got ${val}`);
    assert.strictEqual(typeof val, 'number');
};

export const take_nullable_string = (val) => {
    assert.ok(val !== null && val !== undefined,
        `expected a string, got ${val}`);
    assert.strictEqual(typeof val, 'string');
};

export const test_nullable_exports = () => {
    // Test rust functions that return JsOption — strict: empty == undefined only.
    const nullVal = wasm.rust_return_nullable_null();
    assert.strictEqual(nullVal, undefined,
        `expected undefined from rust_return_nullable_null, got ${nullVal}`);

    const numVal = wasm.rust_return_nullable_value();
    assert.ok(numVal !== null && numVal !== undefined,
        `expected a value from rust_return_nullable_value, got ${numVal}`);
    assert.strictEqual(numVal, 456);

    // Test rust functions that take JsOption
    wasm.rust_take_nullable_null(undefined);
    wasm.rust_take_nullable_value(789);
};
