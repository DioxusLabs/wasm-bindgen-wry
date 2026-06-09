const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const isize_js_identity = a => a;
export const usize_js_identity = a => a;

// `js_works` is imported into Rust as a *synchronous* extern fn and `usize::works`
// (a sync test) calls it without awaiting, so this body must stay synchronous:
// any thrown assertion then travels back through the synchronous IPC round-trip
// as a real failure of the `works` test, rather than a fire-and-forget rejection.
export const js_works = () => {
    assert.strictEqual(wasm.usize_zero(), 0);
    assert.strictEqual(wasm.usize_one(), 1);
    assert.strictEqual(wasm.isize_neg_one(), -1);
    assert.strictEqual(wasm.isize_i32_min(), -2147483648);
    // `isize::MIN` / `usize::MAX` are the native 64-bit width here (`-2^63` /
    // `2^64-1`), not wasm32's 32-bit `-2147483648` / `4294967295`, so those two
    // pointer-width-specific assertions do not apply on wry's native target.
    // The round-trips below still exercise the isize/usize codec.
    // assert.strictEqual(wasm.isize_min(), -2147483648);
    assert.strictEqual(wasm.usize_u32_max(), 4294967295);
    // assert.strictEqual(wasm.usize_max(), 4294967295);

    assert.strictEqual(wasm.isize_rust_identity(0), 0);
    assert.strictEqual(wasm.isize_rust_identity(1), 1);
    assert.strictEqual(wasm.isize_rust_identity(-1), -1);
    assert.strictEqual(wasm.usize_rust_identity(0), 0);
    assert.strictEqual(wasm.usize_rust_identity(1), 1);

    // The wasm32 `isize::MIN` (`-2147483648`) / `usize::MAX` (`4294967295`)
    // identity round-trips assume the 32-bit pointer width and do not apply on
    // wry's native 64-bit target.
    // const usize_max = 4294967295;
    // const isize_min = -2147483648;
    // assert.strictEqual(wasm.isize_rust_identity(isize_min), isize_min);
    // assert.strictEqual(wasm.usize_rust_identity(usize_max), usize_max);

    // wry returns a numeric `Vec<T>` as a plain `Array`, not a typed array
    // backed by linear memory, so the result is compared against a plain
    // `Array` instead of `Uint32Array`/`Int32Array`.
    assert.deepStrictEqual(wasm.usize_slice([]), []);
    assert.deepStrictEqual(wasm.isize_slice([]), []);
    assert.deepStrictEqual(wasm.usize_slice([1, 2]), [1, 2]);
    assert.deepStrictEqual(wasm.isize_slice([1, 2]), [1, 2]);

    // The single-element slices use the wasm32 `isize::MIN` / `usize::MAX`
    // constants, which assume the 32-bit pointer width and do not apply here.
    // assert.deepStrictEqual(wasm.isize_slice([isize_min]), new Int32Array([isize_min]));
    // assert.deepStrictEqual(wasm.usize_slice([usize_max]), new Uint32Array([usize_max]));

    // `usize::works` imports `js_works` as a SYNCHRONOUS extern fn and calls it
    // without awaiting, so this body must stay synchronous and cannot `await` the
    // async exports. A `.then(..)` assertion here would be fire-and-forget (a
    // failure would not fail this test — exactly the latent-rejection trap noted
    // in the project memory), so those two checks are omitted rather than faked.
    // The async-export value codec for numbers is covered, with real awaiting, by
    // `futures.rs` (async_return_2 etc.) and `async_vecs.rs`.
};
