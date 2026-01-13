//! Lazy initialization support for global JavaScript values
//!
//! This module provides types for lazily initializing and caching JavaScript
//! global values, similar to wasm-bindgen's thread_local_v2 support.

use crate::batch::with_runtime;
use core::{mem::ManuallyDrop, panic::Location};

/// A thread-local accessor for lazily initialized JavaScript values.
///
/// This type provides safe access to cached JavaScript global values,
/// ensuring the value is initialized on first access. You can access
/// the value directly via `Deref`.
///
/// # Example
///
/// ```ignore
/// #[wasm_bindgen]
/// extern "C" {
///     #[wasm_bindgen(thread_local_v2, js_name = window)]
///     pub static WINDOW: Window;
/// }
///
/// // Access the cached window value directly
/// let doc = WINDOW.document();
/// ```
pub struct JsThreadLocal<T: 'static> {
    key: ThreadLocalKey<'static>,
    init: fn() -> T,
    phantom: core::marker::PhantomData<T>,
}

impl<T> JsThreadLocal<T> {
    /// Create a new `JsThreadLocal` from a `LocalKey`.
    #[doc(hidden)]
    #[track_caller]
    pub const fn new(init: fn() -> T, index: u32) -> Self {
        let caller = Location::caller();
        Self {
            key: ThreadLocalKey {
                file: caller.file(),
                line: caller.line(),
                column: caller.column(),
                index,
            },
            init,
            phantom: core::marker::PhantomData,
        }
    }

    /// Run a closure with access to the cached value.
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let init = self.init;
        // Check if it exists in the runtime
        let initialized = with_runtime(|runtime| runtime.has_thread_local(self.key));
        if !initialized {
            // Initialize it before we open the runtime borrow
            let value = init();
            with_runtime(|runtime| {
                // We never drop js thread locals because:
                // 1. The destructor only has an effect when the webview still exists and it should now be gone
                // 2. It would rely on the thread local being dropped before the runtime is dropped, which relies on the drop order of
                // different thread locals
                runtime.insert_thread_local(self.key, ManuallyDrop::new(value));
            });
        }
        // Now that we know it exists, access it
        let value: ManuallyDrop<T> = with_runtime(|runtime| runtime.take_thread_local(self.key));
        // We can't hold the runtime borrow while calling f, so we have to
        // move the value out temporarily and put it back afterwards. The f
        // closure could re-enter the runtime to access other thread locals.
        let result = f(&value);
        // Put it back
        with_runtime(|runtime| {
            runtime.insert_thread_local(self.key, value);
        });
        result
    }
}

/// A key used to identify a thread-local variable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ThreadLocalKey<'a> {
    /// The file name
    file: &'a str,

    /// The line number
    line: u32,

    /// The column number
    column: u32,

    /// The index of the signal in the file - used to disambiguate macro calls
    index: u32,
}
