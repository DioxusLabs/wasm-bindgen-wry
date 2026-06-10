//! Lazily-initialized, cached runtime-local JavaScript values.

use alloc::boxed::Box;

use crate::runtime::with_backend;

/// A runtime-local accessor for lazily initialized JavaScript values.
///
/// This type provides safe access to cached JavaScript global values, ensuring
/// the value is initialized on first access in the active runtime.
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
/// WINDOW.with(|window| {
///     let doc = window.document();
/// });
/// ```
pub struct LazyCell<T: 'static> {
    init: fn() -> T,
}

impl<T> LazyCell<T> {
    /// Create a new `LazyCell`.
    #[doc(hidden)]
    pub const fn new(init: fn() -> T) -> Self {
        Self { init }
    }

    /// Return the cached value, initializing it if needed.
    pub fn force(&'static self) -> &'static T {
        let init = self.init;
        // Take the value out of the runtime, initializing it if it isn't there
        // yet. We initialize outside the runtime borrow because init() may
        // re-enter the runtime to access other thread locals.
        //
        // We never drop js thread locals because:
        // 1. The destructor only has an effect when the webview still exists and it should now be gone
        // 2. It would rely on the thread local being dropped before the runtime is dropped, which relies on the drop order of
        // different thread locals
        let value = match with_backend(|runtime| runtime.take_thread_local_box(self)) {
            Some(value) => *value.downcast::<&'static T>().expect("type mismatch"),
            None => Box::leak(Box::new(init())) as &'static T,
        };
        with_backend(|runtime| {
            runtime.insert_thread_local_box(self, Box::new(value));
        });
        value
    }
}

/// Backwards-compatible name used by generated `thread_local_v2` bindings.
pub struct JsThreadLocal<T: 'static> {
    inner: LazyCell<T>,
}

impl<T> JsThreadLocal<T> {
    /// Create a new `JsThreadLocal`.
    #[doc(hidden)]
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            inner: LazyCell::new(init),
        }
    }

    /// Run a closure with access to the cached value.
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        f(LazyCell::force(&self.inner))
    }
}
