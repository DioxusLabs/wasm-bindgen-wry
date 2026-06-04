//! Lazily-initialized, cached thread-local JavaScript values.

use alloc::boxed::Box;
use core::mem::ManuallyDrop;

use crate::runtime::with_backend;

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
    init: fn() -> T,
}

impl<T> JsThreadLocal<T> {
    /// Create a new `JsThreadLocal`.
    #[doc(hidden)]
    pub const fn new(init: fn() -> T) -> Self {
        Self { init }
    }

    /// Run a closure with access to the cached value.
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let init = self.init;
        // Take the value out of the runtime, initializing it if it isn't there
        // yet. We initialize outside the runtime borrow because init() may
        // re-enter the runtime to access other thread locals.
        //
        // We never drop js thread locals because:
        // 1. The destructor only has an effect when the webview still exists and it should now be gone
        // 2. It would rely on the thread local being dropped before the runtime is dropped, which relies on the drop order of
        // different thread locals
        let value: ManuallyDrop<T> =
            match with_backend(|runtime| runtime.take_thread_local_box(self)) {
                Some(value) => *value.downcast::<ManuallyDrop<T>>().expect("type mismatch"),
                None => ManuallyDrop::new(init()),
            };
        // We can't hold the runtime borrow while calling f, so we have to
        // move the value out temporarily and put it back afterwards. The f
        // closure could re-enter the runtime to access other thread locals.
        let result = f(&value);
        // Put it back
        with_backend(|runtime| {
            runtime.insert_thread_local_box(self, Box::new(value));
        });
        result
    }
}
