//! Test-side compatibility shim for running wasm-bindgen's `tests/wasm` suite against the
//! wry-bindgen native harness.
//!
//! `#[wasm_bindgen_test]` (from [`wasm_bindgen_test_macro`]) registers each test into an
//! [`inventory`] collection of [`RegisteredTest`] values. The `upstream_tests` harness
//! iterates that collection to build its test list. The shipping wry-bindgen runtime is
//! untouched: everything needed to run the upstream suite lives in this crate and the
//! test binary.

pub use wasm_bindgen_test_macro::wasm_bindgen_test;

/// Internals referenced by the `#[wasm_bindgen_test]` expansion. Not a stable API.
pub mod __rt {
    pub use inventory;

    /// A single test, as registered by `#[wasm_bindgen_test]`.
    pub struct RegisteredTest {
        /// `module_path!()` of the test function.
        pub module_path: &'static str,
        /// The test function's identifier.
        pub name: &'static str,
        /// `None` for a normal test; `Some(None)` for `#[should_panic]`; `Some(Some(msg))`
        /// for `#[should_panic(expected = "msg")]`.
        pub should_panic: Option<Option<&'static str>>,
        /// Whether the test carried `#[ignore]`.
        pub ignore: bool,
        /// The thunk that runs the test body.
        pub kind: TestKind,
    }

    /// A registered test's runnable body. `Result`-returning and async test functions are
    /// normalized to `-> ()` thunks by the macro.
    pub enum TestKind {
        Sync(fn()),
        Async(fn() -> core::pin::Pin<Box<dyn core::future::Future<Output = ()>>>),
    }

    inventory::collect!(RegisteredTest);

    /// Backing for [`console_log!`]: writes to native stdout, where the harness output is.
    pub fn log(message: String) {
        println!("{message}");
    }

    /// Backing for [`console_error!`]: writes to native stderr.
    pub fn error(message: String) {
        eprintln!("{message}");
    }
}

/// `console_log!`-compatible macro. The test body runs natively, so this logs to stdout.
#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => { $crate::__rt::log(::std::format!($($t)*)) };
}

/// `console_error!`-compatible macro.
#[macro_export]
macro_rules! console_error {
    ($($t:tt)*) => { $crate::__rt::error(::std::format!($($t)*)) };
}

/// Compatibility shim for `wasm_bindgen_test_configure!`. `run_in_browser` is a no-op
/// (the wry webview is a browser); the worker/node/emscripten environments are rejected
/// at compile time, so a file opting into one is never silently mis-run.
#[macro_export]
macro_rules! wasm_bindgen_test_configure {
    (run_in_browser) => {};
    (run_in_node_experimental) => {
        ::core::compile_error!("run_in_node_experimental is unsupported under wry-bindgen");
    };
    (run_in_dedicated_worker) => {
        ::core::compile_error!("run_in_dedicated_worker is unsupported under wry-bindgen");
    };
    (run_in_shared_worker) => {
        ::core::compile_error!("run_in_shared_worker is unsupported under wry-bindgen");
    };
    (run_in_service_worker) => {
        ::core::compile_error!("run_in_service_worker is unsupported under wry-bindgen");
    };
    (run_in_emscripten) => {
        ::core::compile_error!("run_in_emscripten is unsupported under wry-bindgen");
    };
}

/// Mirrors `wasm_bindgen_test::prelude`.
pub mod prelude {
    pub use crate::wasm_bindgen_test;
    pub use crate::{console_error, console_log, wasm_bindgen_test_configure};
}
