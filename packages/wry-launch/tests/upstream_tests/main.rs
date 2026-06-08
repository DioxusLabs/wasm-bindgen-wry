//! Runs wasm-bindgen's upstream `tests/wasm` suite against the wry-bindgen runtime.
//!
//! Each upstream test file is pulled in unmodified via `#[path]`. Its
//! `#[wasm_bindgen_test]` functions register themselves into an `inventory` collection
//! (see the `wasm-bindgen-test` crate); [`build_tests`] turns that collection into the
//! shared harness's `TestCase` list. The upstream suite runs NonBatched only.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::process::ExitCode;

use futures_util::FutureExt;
use wasm_bindgen::wasm_bindgen;
// Re-exported at the crate root so upstream files that reference `crate::JsValue`
// (e.g. `bigint.rs`'s `try_from_works` submodule) resolve it, mirroring how the
// upstream test crate re-exports it.
pub use wasm_bindgen::JsValue;
use wasm_bindgen_test::__rt::{RegisteredTest, TestKind, inventory};

#[path = "../common/harness.rs"]
mod harness;

use harness::{BatchMode, TEST_TIMEOUT, TestBody, TestCase, harness_main, run_with_timeout};

// Test-side `require('assert')` / `require('wasm-bindgen-test')` shims. The transformed
// upstream `.js` modules resolve these lazily — `wasm` reads Rust exports off `window`,
// `assert` reads `globalThis.__wbgAssert`, which this module installs during init.
#[wasm_bindgen(inline_js = r#"
function __wbgDeepEqual(a, b, strict) {
    if (a === b) return true;
    if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') {
        return strict ? a === b : a == b;
    }
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every(k => __wbgDeepEqual(a[k], b[k], strict));
}
function __wbgCaught(fn) {
    try { fn(); } catch (e) { return e; }
    return undefined;
}
function __wbgMatch(err, expected) {
    if (expected === undefined) return;
    const s = err && err.message !== undefined ? String(err.message) : String(err);
    if (expected instanceof RegExp && !expected.test(s)) {
        throw new Error('error "' + s + '" did not match ' + expected);
    }
}
// `assert` is callable (assert(value, message)) AND carries the method API, matching Node.
function __wbgAssert(v, m) { if (!v) throw new Error(m || ('assert: ' + String(v) + ' is not truthy')); }
__wbgAssert.strictEqual = (a, b, m) => { if (a !== b) throw new Error(m || ('strictEqual: ' + String(a) + ' !== ' + String(b))); };
__wbgAssert.notStrictEqual = (a, b, m) => { if (a === b) throw new Error(m || 'notStrictEqual'); };
__wbgAssert.equal = (a, b, m) => { if (a != b) throw new Error(m || ('equal: ' + String(a) + ' != ' + String(b))); };
__wbgAssert.ok = (v, m) => { if (!v) throw new Error(m || 'ok: value is falsy'); };
__wbgAssert.deepStrictEqual = (a, b, m) => { if (!__wbgDeepEqual(a, b, true)) throw new Error(m || 'deepStrictEqual'); };
__wbgAssert.deepEqual = (a, b, m) => { if (!__wbgDeepEqual(a, b, false)) throw new Error(m || 'deepEqual'); };
__wbgAssert.throws = (fn, expected, m) => { const e = __wbgCaught(fn); if (e === undefined) throw new Error(m || 'missing expected exception'); __wbgMatch(e, expected); };
__wbgAssert.doesNotThrow = (fn, m) => { const e = __wbgCaught(fn); if (e !== undefined) throw new Error(m || ('unexpected exception: ' + e)); };
__wbgAssert.rejects = async (p, expected, m) => { let err; try { await (typeof p === 'function' ? p() : p); } catch (e) { err = e; if (err === undefined) err = new Error('rejected with undefined'); } if (err === undefined) throw new Error(m || 'missing expected rejection'); __wbgMatch(err, expected); };
__wbgAssert.doesNotReject = async (p, m) => { try { await (typeof p === 'function' ? p() : p); } catch (e) { throw new Error(m || ('unexpected rejection: ' + e)); } };
__wbgAssert.match = (s, re, m) => { if (!re.test(s)) throw new Error(m || ('match: ' + s + ' !~ ' + re)); };
__wbgAssert.fail = (m) => { throw new Error(m || 'fail'); };
globalThis.__wbgAssert = __wbgAssert;
export function __wbg_upstream_test_init() {}
"#)]
extern "C" {
    fn __wbg_upstream_test_init();
}

// Enabled: every upstream tests/wasm file except those whose .js uses require/global.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/3944.rs"]
mod _3944;
// `duplicate_deps.rs`: two fixture crates (`wasm-bindgen-test-crate-a`/`-b`) each
// import a `foo` binding from the *same* JS module (`duplicate_deps.js`). Because
// the runtime keys modules by content hash, both crates' identical module content
// dedups to one loaded module, exercising that two crates can share a JS dependency.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/duplicate_deps.rs"]
mod duplicate_deps;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/duplicates.rs"]
mod duplicates;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/final.rs"]
mod final_test;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/ignore.rs"]
mod ignore;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/inner_self.rs"]
mod inner_self;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/intrinsics.rs"]
mod intrinsics;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/js_keywords.rs"]
mod js_keywords;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/js_objects.rs"]
mod js_objects;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/js_vec.rs"]
mod js_vec;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/jscast.rs"]
mod jscast;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/math.rs"]
mod math;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/option.rs"]
mod option;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/reexport.rs"]
mod reexport;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/rethrow.rs"]
mod rethrow;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/should_panic.rs"]
mod should_panic;
// `slice.rs`: `&[T]`/`Vec<T>`/`Box<[T]>`/`&mut [T]` codecs for every primitive
// element kind, plus their `MaybeUninit<T>` and `Clamped<..>` wrappers. A
// `MaybeUninit<T>` element rides the wire as a bare `T` (callers always
// initialize it). A `&mut [T]` argument is written back over the wire — wry has
// no shared linear memory, so the mutated array travels back appended to the
// response and is copied into the caller's slice. This works in both directions:
// a Rust export's `&mut [T]` arg (`export_mut`) and a JS import's `&mut [T]` arg
// (`import_mut`). No sub-case is wasm-specific here — `slice.js` was already
// adapted CJS->ESM and exercises only data round-trips, so every test runs.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/slice.rs"]
mod slice;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/slice_jsvalue.rs"]
mod slice_jsvalue;
// `slice_to_array.rs`: the `#[wasm_bindgen(slice_to_array)]` attribute (per-fn
// and block-level). In wry-bindgen a `&[T]`/`Option<&[T]>` argument already
// rides the boundary as a plain JS `Array` (there is no typed-array wire path),
// so the attribute parses and the slice arrives on the JS side as an `Array` —
// exactly the shape `slice_to_array` requests. All element kinds (primitives,
// `String`, imported types) and the mixed-arg method form run.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/slice_to_array.rs"]
mod slice_to_array;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/string_vecs.rs"]
mod string_vecs;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/structural.rs"]
mod structural;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/char.rs"]
mod char;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/nullable.rs"]
mod nullable;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/node.rs"]
mod node;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/truthy_falsy.rs"]
mod truthy_falsy;
// `usize.rs`: the isize/usize codec round-trips run; the companion `js_works` is
// kept synchronous (it is imported as a sync extern fn and `works` calls it
// without awaiting) so assertion failures travel back as real failures. The
// wasm32-only sub-cases are skipped in `usize.js`: `isize::MIN`/`usize::MAX`
// assume the 32-bit pointer width (native is 64-bit), and numeric `Vec<T>`
// returns as a plain `Array` rather than an `Int32Array`/`Uint32Array`.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/usize.rs"]
mod usize;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/variadic.rs"]
mod variadic;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/result_jserror.rs"]
mod result_jserror;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/arg_names.rs"]
mod arg_names;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/bigint.rs"]
mod bigint;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/js_namespace_exports.rs"]
mod js_namespace_exports;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/unwind.rs"]
mod unwind;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/result.rs"]
mod result;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/struct_vecs.rs"]
mod struct_vecs;
// `optional_primitives.js` has its `isize`/`usize` MIN/MAX and 32-bit-wraparound
// assertions skipped: those hardcode the wasm32 pointer width, while wry's
// `isize`/`usize` are the native 64-bit width. The small-value round-trips
// (none/zero/one/neg_one) and every other primitive still run.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/optional_primitives.rs"]
mod optional_primitives;
// `classes.rs`: the full exported-class surface — constructors (incl. renamed
// `#[wasm_bindgen(constructor)]`), static/instance methods, public/readonly/skip
// fields, `getter_with_clone`, `js_name`/`js_class` renames (struct, fields,
// methods), empty/macro-defined classes, `Option<Class>`, `inspectable`
// (`toJSON`/`toString`, overridable), and `#[wasm_bindgen(this)]` free functions.
// Shared-borrow semantics in the object store let the same object ride as both a
// `&self` receiver and a `&T`/`&mut T` argument (`x.foo(x)`), with a `&mut`
// alias reporting "recursive use of an object" and a wrong-class argument
// "expected instance of <Class>"; a double `free()` reports
// "null pointer passed to rust". Excluded sub-cases (companion `classes.js`):
// the `cfg_attr(target_family="wasm")` `ConditionalSkip`/`ConditionalBindings`
// classes never get `#[wasm_bindgen]` on the native target (wasm-specific); the
// nodejs `console.log`-formatting checks use `process.stdout`/`console.Console`;
// and one `b.free()` assertion depends on wasm-bindgen leaving a dangling
// `RefMut` after a failed `borrow_mut` (wry unwinds that borrow cleanly).
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/classes.rs"]
mod classes;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/enum_vecs.rs"]
mod enum_vecs;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/macro_rules.rs"]
mod macro_rules;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/generics.rs"]
mod generics;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/try_from_js_value.rs"]
mod try_from_js_value;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/gc.rs"]
mod gc;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/enums.rs"]
mod enums;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/getters_and_setters.rs"]
mod getters_and_setters;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/vendor_prefix.rs"]
mod vendor_prefix;
// Async exports that resolve to a value work now: the returned `Promise`'s heap
// reference transfers to JS (Rust forgets it, JS takes ownership on decode). Two
// wry-platform-specific sub-cases are skipped in the companion `.js` (not the
// whole file): a `&mut [T]` argument is not written back into the caller's typed
// array (no shared linear memory), and a numeric `Vec<T>` returns as a plain
// `Array` rather than a typed array. See the comments in `futures.js`/`async_vecs.js`.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/futures.rs"]
mod futures;
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/async_vecs.rs"]
mod async_vecs;
// `closures.rs` largely compiles now (closure variance/upcasts, 8-arity, value
// upcasts added). The remaining gap is the "reference as first argument" family
// — `&dyn Fn(&T)` / `&mut dyn FnMut(&T)` passed to JS, and exported-struct
// `&T` closure args. wry encodes closures with blanket trait impls
// (`impl<A> BinaryEncode for &dyn Fn(A)`); a ref-arg impl (`&dyn Fn(&First)`)
// overlaps that blanket at `&'static First`, which coherence forbids. Supporting
// it needs a `describe`-style per-instance closure codegen like wasm-bindgen's,
// rather than blanket impls. Quarantined until that restructure lands.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/closures.rs"]
mod closures;
// `simple.rs`: the number/string/option/instanceof/typeof round-trips and the
// `externref_heap_live_count` accounting all run. Raw pointers and `NonNull`
// ride the boundary as their native-word address (encoded like `usize`), so
// every export compiles. Four sub-cases are skipped in `simple.js` (not the
// whole file), each a wasm/nodejs intrinsic with no native analogue:
// `test_wrong_types` (gated on `require('process').env`), `test_raw_pointers`
// and `test_non_null` (inspect `__wasm.memory.buffer`, wasm linear memory), and
// `test_other_exports_still_available` (reaches the raw `__wasm` instance
// exports for a `#[no_mangle]` symbol).
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/simple.rs"]
mod simple;
// `validate_prt.rs`: moved-value validation. An exported struct passed or
// returned by value advertises the `RustValue` wire tag, so JS zeroes the
// wrapper's handle on a by-value pass (`eat`) and `self`-consuming members
// (`rot`, by-value getter/setter) zero `this.__handle` after the call; a later
// use throws "Attempt to use a moved value". The companion `.js`'s nodejs-only
// `process.env` debug gate is read through `globalThis` (unset here, so the
// debug message applies).
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/validate_prt.rs"]
mod validate_prt;
// `imports.rs`: free-function/static/namespace imports, rust-keyword and
// special-character `js_name`s (`pub`, `baz$`, `kebab-case`, string-literal
// breakers), `js_namespace` statics, two-module same-`js_name`/namespace
// disambiguation, and undefined-import tolerance. The companion `imports.js` is
// adapted CJS->ESM (string-named exports for the non-identifier `js_name`s). One
// sub-case is skipped in the companion (not the whole file):
// `assert_dead_import_not_generated` reads the emitted bindings file off disk via
// nodejs `require.resolve`/`fs` to check tree-shaking, a build-time codegen
// artifact property with no runtime analogue.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/imports.rs"]
mod imports;
// `import_class.rs`: imported-class bindings — namespaced static functions,
// constructors, instance/static methods, getters/setters (including
// `kebab-case` string names), `js_class`/`js_name` renames (`default`),
// `catch` constructors, structural statics, and nested-namespace statics
// (`js_namespace = ["nestedNamespace", "InnerClass"]` reachable as
// `InnerClass::inner_static_function`). The companion `import_class.js` is
// adapted CJS->ESM; `export const default` (a syntax error) becomes the
// canonical `export default`, exposing the same `module.default` binding that
// `js_class = default` references.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/import_class.rs"]
mod import_class;
// `no_shims.rs`: imports whose argument/return conversions need no JS shim
// (primitives, `bool`, `JsValue`, a `js_namespace` namespace object). The inline
// JS was adapted CJS->ESM (`module.exports.X` -> `export const X`, the
// `MyNamespace` object built as an object literal). `assert_no_shim` is a codegen
// property wry already satisfies (it parses and compiles without a shim).
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/no_shims.rs"]
mod no_shims;
// `inheritance.rs`: `#[wasm_bindgen(extends = ...)]` exported-struct inheritance.
// Each descendant publishes its inherited ancestors as separate handles backed by
// a `Parent<Ancestor>` (a clone of the shared parent cell), so an inherited
// ancestor method dispatched on a descendant operates on the ancestor's shared
// data via per-class `__handle_<Ancestor>` slots. The `super(__wbgSuperSkip)`
// sentinel short-circuits the parent's generated constructor; subclass-dispatch
// gates reject feeding a descendant to an ancestor's consuming/free/by-value
// shim; the topo sort emits parents before children.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/inheritance.rs"]
mod inheritance;
// `api.rs`: the `JsValue` surface — construction from `&str`/`f64`/`bool`,
// `null`/`undefined` and the `is_null`/`is_undefined`/`is_null_or_undefined`
// predicates, symbol creation/detection, `as_string`/`as_bool`/`as_f64`,
// equality (`==`, NaN, object identity), `#[wasm_bindgen(variadic)]` exports, and
// `Debug` formatting of every `JsValue` shape (`debug_output`). The companion
// `api.js` was already adapted CJS->ESM. Three `#[wasm_bindgen_test]`s are
// genuinely wasm-specific and excluded in `build_tests` (they call
// `wasm_bindgen::memory()`/`instance()`/`exports()`/`function_table()`, which
// introspect wasm linear memory / the `WebAssembly.Instance` / the wasm function
// table — concepts with no analogue on the native wry target).
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/api.rs"]
mod api;
// `link_to.rs`: `link_to!(module/inline_js/raw_module = ...)` registers JS with
// the runtime and returns its `/__wbg__/snippets/{hash}.js` URL; the companion
// `link_to.js` fetches that URL via synchronous XHR. A `raw_module` specifier is
// returned verbatim and (being unregistered) fails the fetch.
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/link_to.rs"]
mod link_to;


fn build_tests() -> Vec<TestCase> {
    // Ensure the assert/exports shim module is loaded before any test runs.
    __wbg_upstream_test_init();

    let mut tests = Vec::new();
    for reg in inventory::iter::<RegisteredTest>() {
        if reg.ignore {
            continue;
        }
        // wasm-specific: these `api.rs` tests introspect the running wasm
        // instance itself — `wasm_bindgen::memory()`/`instance()`/`exports()`
        // (the `WebAssembly.Memory`/`WebAssembly.Instance`) and
        // `wasm_bindgen::function_table()` (the wasm indirect function table).
        // None of those exist on the native wry target, so the calls panic;
        // they are N/A here (rule B) and excluded.
        if matches!(
            reg.name,
            "memory_accessor_appears_to_work"
                | "instance_accessor_appears_to_work"
                | "function_table_is"
        ) && reg.module_path.ends_with("::api")
        {
            continue;
        }
        let name = format!("{}::{}", reg.module_path, reg.name);
        let should_panic = reg.should_panic;
        let body: TestBody = match &reg.kind {
            TestKind::Sync(f) => {
                let f = *f;
                Box::new(move || {
                    Box::pin(run_with_timeout(
                        async move { run_sync(f, should_panic) },
                        BatchMode::NonBatched,
                        TEST_TIMEOUT,
                    ))
                })
            }
            TestKind::Async(f) => {
                let f = *f;
                Box::new(move || {
                    Box::pin(run_with_timeout(
                        run_async(f, should_panic),
                        BatchMode::NonBatched,
                        TEST_TIMEOUT,
                    ))
                })
            }
        };
        tests.push(TestCase { name, body });
    }
    tests
}

fn run_sync(f: fn(), should_panic: Option<Option<&'static str>>) {
    match should_panic {
        None => f(),
        Some(expected) => assert_panicked(catch_unwind(f), expected),
    }
}

async fn run_async(
    f: fn() -> Pin<Box<dyn Future<Output = ()>>>,
    should_panic: Option<Option<&'static str>>,
) {
    match should_panic {
        None => f().await,
        Some(expected) => assert_panicked(AssertUnwindSafe(f()).catch_unwind().await, expected),
    }
}

/// Enforce `#[should_panic]`: the test must have panicked, and (when an `expected`
/// substring was given) the panic message must contain it. Panicking here turns into a
/// normal harness failure.
fn assert_panicked(result: std::thread::Result<()>, expected: Option<&str>) {
    let payload = match result {
        Ok(()) => panic!("test did not panic as expected"),
        Err(payload) => payload,
    };
    if let Some(expected) = expected {
        let message = if let Some(s) = payload.downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            String::new()
        };
        assert!(
            message.contains(expected),
            "panic message `{message}` did not contain expected `{expected}`"
        );
    }
}

fn main() -> ExitCode {
    harness_main(build_tests)
}
