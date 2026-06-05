const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

class MyType {
}

export { MyType };

export const take_none_byval = x => {
    assert.strictEqual(x, undefined);
};
export const take_some_byval = x => {
    assert.ok(x !== null && x !== undefined);
    assert.ok(x instanceof MyType);
};
export const return_undef_byval = () => undefined;
export const return_null_byval = () => null;
export const return_some_byval = () => new MyType();

export const test_option_values = () => {
    wasm.rust_take_none_byval(null);
    wasm.rust_take_none_byval(undefined);
    wasm.rust_take_some_byval(new MyType());
    assert.strictEqual(wasm.rust_return_none_byval(), undefined);
    const x = wasm.rust_return_some_byval();
    assert.ok(x !== null && x !== undefined);
    assert.ok(x instanceof MyType);
};

export const take_option_jsvalue_none = x => {
    assert.strictEqual(x, undefined);
};

export const take_option_jsvalue_some = x => {
    assert.ok(x !== null && x !== undefined);
};

export const return_option_jsvalue_none = () => undefined;

export const return_option_jsvalue_some = () => "js value";

export const test_option_jsvalue_values = () => {
    wasm.rust_take_option_jsvalue_none(null);
    wasm.rust_take_option_jsvalue_none(undefined);
    wasm.rust_take_option_jsvalue_some("test");
    wasm.rust_take_option_jsvalue_some(42);
    wasm.rust_take_option_jsvalue_some({obj: "value"});
    
    assert.strictEqual(wasm.rust_return_option_jsvalue_none(), undefined);
    const val = wasm.rust_return_option_jsvalue_some();
    assert.ok(val !== null && val !== undefined);
    assert.strictEqual(val, "rust value");
};
