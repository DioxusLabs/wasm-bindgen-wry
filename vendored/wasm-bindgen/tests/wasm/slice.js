const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

const isWasm64 = () => typeof wasm.wasm64_return_usize === 'function';

const pointerSizedSignedArray = values =>
    isWasm64() ? new BigInt64Array(values.map(BigInt)) : new Int32Array(values);

const pointerSizedUnsignedArray = values =>
    isWasm64() ? new BigUint64Array(values.map(BigInt)) : new Uint32Array(values);

const typedValue = (array, value) =>
    typeof array[0] === 'bigint' ? BigInt(value) : value;

const hasMaybeUninitBindings = () => typeof wasm.export_uninit_i8 === 'function';

const assertUninitExport = (name, value) => {
    if (!hasMaybeUninitBindings()) return;
    assert.deepStrictEqual(wasm[`export_uninit_${name}`](value), value);
    assert.deepStrictEqual(wasm[`export_optional_uninit_${name}`](value), value);
};

const assertUninitOptionalUndefined = name => {
    if (!hasMaybeUninitBindings()) return;
    assert.strictEqual(wasm[`export_optional_uninit_${name}`](undefined), undefined);
};

const assertUninitImport = (name, value) => {
    if (!hasMaybeUninitBindings()) return;
    assert.deepStrictEqual(wasm[`import_rust_uninit_${name}`](value), value);
};

const passUninitArray = (name, value) => {
    if (!hasMaybeUninitBindings()) return;
    wasm[`pass_array_rust_uninit_${name}`](value);
};

const exportUninitMutRun = (value, name) => {
    if (!hasMaybeUninitBindings()) return;
    export_mut_run(value, wasm[`export_mut_uninit_${name}`]);
};

export const js_export = () => {
    const i8 = new Int8Array(2);
    i8[0] = 1;
    i8[1] = 2;
    assert.deepStrictEqual(wasm.export_i8(i8), i8);
    assert.deepStrictEqual(wasm.export_optional_i8(i8), i8);
    assertUninitExport('i8', i8);
    const u8 = new Uint8Array(2);
    u8[0] = 1;
    u8[1] = 2;
    assert.deepStrictEqual(wasm.export_u8(u8), u8);
    assert.deepStrictEqual(wasm.export_optional_u8(u8), u8);
    assertUninitExport('u8', u8);

    const i16 = new Int16Array(2);
    i16[0] = 1;
    i16[1] = 2;
    assert.deepStrictEqual(wasm.export_i16(i16), i16);
    assert.deepStrictEqual(wasm.export_optional_i16(i16), i16);
    assertUninitExport('i16', i16);
    const u16 = new Uint16Array(2);
    u16[0] = 1;
    u16[1] = 2;
    assert.deepStrictEqual(wasm.export_u16(u16), u16);
    assert.deepStrictEqual(wasm.export_optional_u16(u16), u16);
    assertUninitExport('u16', u16);

    const i32 = new Int32Array(2);
    i32[0] = 1;
    i32[1] = 2;
    assert.deepStrictEqual(wasm.export_i32(i32), i32);
    assert.deepStrictEqual(wasm.export_optional_i32(i32), i32);
    assertUninitExport('i32', i32);

    const isize = pointerSizedSignedArray([1, 2]);
    assert.deepStrictEqual(wasm.export_isize(isize), isize);
    assert.deepStrictEqual(wasm.export_optional_isize(isize), isize);
    assertUninitExport('isize', isize);

    const u32 = new Uint32Array(2);
    u32[0] = 1;
    u32[1] = 2;
    assert.deepStrictEqual(wasm.export_u32(u32), u32);
    assert.deepStrictEqual(wasm.export_optional_u32(u32), u32);
    assertUninitExport('u32', u32);

    const usize = pointerSizedUnsignedArray([1, 2]);
    assert.deepStrictEqual(wasm.export_usize(usize), usize);
    assert.deepStrictEqual(wasm.export_optional_usize(usize), usize);
    assertUninitExport('usize', usize);

    const f32 = new Float32Array(2);
    f32[0] = 1;
    f32[1] = 2;
    assert.deepStrictEqual(wasm.export_f32(f32), f32);
    assert.deepStrictEqual(wasm.export_optional_f32(f32), f32);
    assertUninitExport('f32', f32);
    const f64 = new Float64Array(2);
    f64[0] = 1;
    f64[1] = 2;
    assert.deepStrictEqual(wasm.export_f64(f64), f64);
    assert.deepStrictEqual(wasm.export_optional_f64(f64), f64);
    assertUninitExport('f64', f64);

    assert.strictEqual(wasm.export_optional_i8(undefined), undefined);
    assert.strictEqual(wasm.export_optional_u8(undefined), undefined);
    assert.strictEqual(wasm.export_optional_i16(undefined), undefined);
    assert.strictEqual(wasm.export_optional_u16(undefined), undefined);
    assert.strictEqual(wasm.export_optional_i32(undefined), undefined);
    assert.strictEqual(wasm.export_optional_isize(undefined), undefined);
    assert.strictEqual(wasm.export_optional_u32(undefined), undefined);
    assert.strictEqual(wasm.export_optional_usize(undefined), undefined);
    assert.strictEqual(wasm.export_optional_f32(undefined), undefined);
    assert.strictEqual(wasm.export_optional_f64(undefined), undefined);

    assertUninitOptionalUndefined('i8');
    assertUninitOptionalUndefined('u8');
    assertUninitOptionalUndefined('i16');
    assertUninitOptionalUndefined('u16');
    assertUninitOptionalUndefined('i32');
    assertUninitOptionalUndefined('isize');
    assertUninitOptionalUndefined('u32');
    assertUninitOptionalUndefined('usize');
    assertUninitOptionalUndefined('f32');
    assertUninitOptionalUndefined('f64');
};

const test_import = (a, b, c) => {
    const expectedOne = typeof a[0] === 'bigint' ? 1n : 1;
    const expectedTwo = typeof a[1] === 'bigint' ? 2n : 2;
    assert.strictEqual(a.length, 2);
    assert.strictEqual(a[0], expectedOne);
    assert.strictEqual(a[1], expectedTwo);
    assert.strictEqual(b.length, 2);
    assert.strictEqual(b[0], expectedOne);
    assert.strictEqual(b[1], expectedTwo);
    assert.strictEqual(c, undefined);
    return a;
};

export const import_js_i8 = test_import;
export const import_js_u8 = test_import;
export const import_js_i16 = test_import;
export const import_js_u16 = test_import;
export const import_js_i32 = test_import;
export const import_js_isize = test_import;
export const import_js_u32 = test_import;
export const import_js_usize = test_import;
export const import_js_f32 = test_import;
export const import_js_f64 = test_import;

export const import_js_uninit_i8 = test_import;
export const import_js_uninit_u8 = test_import;
export const import_js_uninit_i16 = test_import;
export const import_js_uninit_u16 = test_import;
export const import_js_uninit_i32 = test_import;
export const import_js_uninit_isize = test_import;
export const import_js_uninit_u32 = test_import;
export const import_js_uninit_usize = test_import;
export const import_js_uninit_f32 = test_import;
export const import_js_uninit_f64 = test_import;

export const js_import = () => {
    const i8 = new Int8Array(2);
    i8[0] = 1;
    i8[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_i8(i8), i8);
    assertUninitImport('i8', i8);
    const u8 = new Uint8Array(2);
    u8[0] = 1;
    u8[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_u8(u8), u8);
    assertUninitImport('u8', u8);

    const i16 = new Int16Array(2);
    i16[0] = 1;
    i16[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_i16(i16), i16);
    assertUninitImport('i16', i16);
    const u16 = new Uint16Array(2);
    u16[0] = 1;
    u16[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_u16(u16), u16);
    assertUninitImport('u16', u16);

    const i32 = new Int32Array(2);
    i32[0] = 1;
    i32[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_i32(i32), i32);
    assertUninitImport('i32', i32);

    const isize = pointerSizedSignedArray([1, 2]);
    assert.deepStrictEqual(wasm.import_rust_isize(isize), isize);
    assertUninitImport('isize', isize);

    const u32 = new Uint32Array(2);
    u32[0] = 1;
    u32[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_u32(u32), u32);
    assertUninitImport('u32', u32);

    const usize = pointerSizedUnsignedArray([1, 2]);
    assert.deepStrictEqual(wasm.import_rust_usize(usize), usize);
    assertUninitImport('usize', usize);

    const f32 = new Float32Array(2);
    f32[0] = 1;
    f32[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_f32(f32), f32);
    assertUninitImport('f32', f32);
    const f64 = new Float64Array(2);
    f64[0] = 1;
    f64[1] = 2;
    assert.deepStrictEqual(wasm.import_rust_f64(f64), f64);
    assertUninitImport('f64', f64);
};

export const js_pass_array = () => {
    wasm.pass_array_rust_i8([1, 2]);
    wasm.pass_array_rust_u8([1, 2]);
    wasm.pass_array_rust_i16([1, 2]);
    wasm.pass_array_rust_u16([1, 2]);
    wasm.pass_array_rust_i32([1, 2]);
    wasm.pass_array_rust_u32([1, 2]);
    wasm.pass_array_rust_isize(pointerSizedSignedArray([1, 2]));
    wasm.pass_array_rust_usize(pointerSizedUnsignedArray([1, 2]));
    wasm.pass_array_rust_f32([1, 2]);
    wasm.pass_array_rust_f64([1, 2]);

    passUninitArray('i8', [1, 2]);
    passUninitArray('u8', [1, 2]);
    passUninitArray('i16', [1, 2]);
    passUninitArray('u16', [1, 2]);
    passUninitArray('i32', [1, 2]);
    passUninitArray('u32', [1, 2]);
    passUninitArray('isize', pointerSizedSignedArray([1, 2]));
    passUninitArray('usize', pointerSizedUnsignedArray([1, 2]));
    passUninitArray('f32', [1, 2]);
    passUninitArray('f64', [1, 2]);
};

const import_mut_foo = (a, b, c) => {
    const one = typedValue(a, 1);
    const two = typedValue(a, 2);
    const four = typedValue(a, 4);
    const five = typedValue(a, 5);
    const six = typedValue(b, 6);
    const seven = typedValue(b, 7);
    const eight = typedValue(b, 8);
    assert.strictEqual(a.length, 3);
    assert.strictEqual(a[0], one);
    assert.strictEqual(a[1], two);
    a[0] = four;
    a[1] = five;
    assert.strictEqual(b.length, 3);
    assert.strictEqual(b[0], four);
    assert.strictEqual(b[1], five);
    assert.strictEqual(b[2], six);
    b[0] = eight;
    b[1] = seven;
    assert.strictEqual(c, undefined);
};

export const import_mut_js_i8 = import_mut_foo;
export const import_mut_js_u8 = import_mut_foo;
export const import_mut_js_i16 = import_mut_foo;
export const import_mut_js_u16 = import_mut_foo;
export const import_mut_js_i32 = import_mut_foo;
export const import_mut_js_u32 = import_mut_foo;
export const import_mut_js_isize = import_mut_foo;
export const import_mut_js_usize = import_mut_foo;
export const import_mut_js_f32 = import_mut_foo;
export const import_mut_js_f64 = import_mut_foo;

export const import_mut_js_uninit_i8 = import_mut_foo;
export const import_mut_js_uninit_u8 = import_mut_foo;
export const import_mut_js_uninit_i16 = import_mut_foo;
export const import_mut_js_uninit_u16 = import_mut_foo;
export const import_mut_js_uninit_i32 = import_mut_foo;
export const import_mut_js_uninit_u32 = import_mut_foo;
export const import_mut_js_uninit_isize = import_mut_foo;
export const import_mut_js_uninit_usize = import_mut_foo;
export const import_mut_js_uninit_f32 = import_mut_foo;
export const import_mut_js_uninit_f64 = import_mut_foo;

const export_mut_run = (a, rust) => {
    const one = typedValue(a, 1);
    const two = typedValue(a, 2);
    const three = typedValue(a, 3);
    const four = typedValue(a, 4);
    const five = typedValue(a, 5);
    assert.strictEqual(a.length, 3);
    a[0] = one;
    a[1] = two;
    a[2] = three;
    console.log(a);
    rust(a);
    console.log(a);
    assert.strictEqual(a.length, 3);
    assert.strictEqual(a[0], four);
    assert.strictEqual(a[1], five);
    assert.strictEqual(a[2], three);
};

export const js_export_mut = () => {
    export_mut_run(new Int8Array(3), wasm.export_mut_i8);
    export_mut_run(new Uint8Array(3), wasm.export_mut_u8);
    export_mut_run(new Int16Array(3), wasm.export_mut_i16);
    export_mut_run(new Uint16Array(3), wasm.export_mut_u16);
    export_mut_run(new Int32Array(3), wasm.export_mut_i32);
    export_mut_run(new Uint32Array(3), wasm.export_mut_u32);
    export_mut_run(pointerSizedSignedArray([0, 0, 0]), wasm.export_mut_isize);
    export_mut_run(pointerSizedUnsignedArray([0, 0, 0]), wasm.export_mut_usize);
    export_mut_run(new Float32Array(3), wasm.export_mut_f32);
    export_mut_run(new Float64Array(3), wasm.export_mut_f64);

    exportUninitMutRun(new Int8Array(3), 'i8');
    exportUninitMutRun(new Uint8Array(3), 'u8');
    exportUninitMutRun(new Int16Array(3), 'i16');
    exportUninitMutRun(new Uint16Array(3), 'u16');
    exportUninitMutRun(new Int32Array(3), 'i32');
    exportUninitMutRun(new Uint32Array(3), 'u32');
    exportUninitMutRun(pointerSizedSignedArray([0, 0, 0]), 'isize');
    exportUninitMutRun(pointerSizedUnsignedArray([0, 0, 0]), 'usize');
    exportUninitMutRun(new Float32Array(3), 'f32');
    exportUninitMutRun(new Float64Array(3), 'f64');
};

export const js_return_vec = () => {
    const app = wasm.return_vec_web_main();

    for (let i = 0; i < 10; i++) {
        app.tick();
        const bad = wasm.return_vec_broken_vec();
        console.log('Received from rust:', i, bad);
        assert.strictEqual(bad[0], 1);
        assert.strictEqual(bad[1], 2);
        assert.strictEqual(bad[2], 3);
        assert.strictEqual(bad[3], 4);
        assert.strictEqual(bad[4], 5);
        assert.strictEqual(bad[5], 6);
        assert.strictEqual(bad[6], 7);
        assert.strictEqual(bad[7], 8);
        assert.strictEqual(bad[8], 9);
    }
};

export const js_clamped = (a, offset) => {
  assert.ok(a instanceof Uint8ClampedArray);
  assert.equal(a.length, 3);
  assert.equal(a[0], offset + 0);
  assert.equal(a[1], offset + 1);
  assert.equal(a[2], offset + 2);
};
