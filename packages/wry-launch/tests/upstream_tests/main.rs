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
#[path = "../../../../vendored/wasm-bindgen/tests/wasm/slice_jsvalue.rs"]
mod slice_jsvalue;
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


fn build_tests() -> Vec<TestCase> {
    // Ensure the assert/exports shim module is loaded before any test runs.
    __wbg_upstream_test_init();

    let mut tests = Vec::new();
    for reg in inventory::iter::<RegisteredTest>() {
        if reg.ignore {
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
