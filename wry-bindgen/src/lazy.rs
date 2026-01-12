//! Lazy initialization support for global JavaScript values
//!
//! This module provides types for lazily initializing and caching JavaScript
//! global values, similar to wasm-bindgen's thread_local_v2 support.

use core::mem::ManuallyDrop;
use std::thread::LocalKey;

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
    // We never drop js thread locals because:
    // 1. The destructor only has an effect when the webview still exists and it should now be gone
    // 2. It would rely on the thread local being dropped before the runtime is dropped, which relies on the drop order of
    // different thread locals
    inner: &'static LocalKey<ManuallyDrop<T>>,
}

impl<T> JsThreadLocal<T> {
    /// Create a new `JsThreadLocal` from a `LocalKey`.
    #[doc(hidden)]
    pub const fn new(inner: &'static LocalKey<ManuallyDrop<T>>) -> Self {
        Self { inner }
    }

    /// Run a closure with access to the cached value.
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        self.inner.with(|val| f(val))
    }
}
