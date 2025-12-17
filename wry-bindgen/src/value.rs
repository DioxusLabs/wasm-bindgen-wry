//! JsValue - An opaque reference to a JavaScript value
//!
//! This type represents a reference to a JavaScript value on the JS heap.
//! API compatible with wasm-bindgen's JsValue.

use std::fmt;

use crate::function::JSFunction;

/// Reserved function ID for dropping heap refs on JS side.
/// This should be handled specially in the JS runtime.
pub const DROP_HEAP_REF_FN_ID: u32 = 0xFFFFFFFF;

/// Reserved function ID for cloning heap refs on JS side.
/// Returns a new heap ID for the cloned value.
pub const CLONE_HEAP_REF_FN_ID: u32 = 0xFFFFFFFE;

/// Offset for reserved JS value indices.
/// Values below JSIDX_RESERVED are special constants that don't need drop/clone.
pub(crate) const JSIDX_OFFSET: u64 = 128;

/// Index for the `undefined` JS value.
pub(crate) const JSIDX_UNDEFINED: u64 = JSIDX_OFFSET;

/// Index for the `null` JS value.
pub(crate) const JSIDX_NULL: u64 = JSIDX_OFFSET + 1;

/// Index for the `true` JS value.
pub(crate) const JSIDX_TRUE: u64 = JSIDX_OFFSET + 2;

/// Index for the `false` JS value.
pub(crate) const JSIDX_FALSE: u64 = JSIDX_OFFSET + 3;

/// First usable heap ID. IDs below this are reserved for special values.
pub(crate) const JSIDX_RESERVED: u64 = JSIDX_OFFSET + 4;

/// An opaque reference to a JavaScript heap object.
///
/// This type is the wry-bindgen equivalent of wasm-bindgen's `JsValue`.
/// It represents any JavaScript value and is used as the base type for
/// all imported JS types.
///
/// JsValue is intentionally opaque - you cannot inspect or create values
/// directly. All values come from JavaScript via the IPC protocol.
///
/// Unlike wasm-bindgen which runs in a single-threaded Wasm environment,
/// this implementation uses the IPC protocol to communicate with JS.
pub struct JsValue {
    idx: u64,
}

impl JsValue {
    /// The `null` JS value constant.
    pub const NULL: JsValue = JsValue::_new(JSIDX_NULL);

    /// The `undefined` JS value constant.
    pub const UNDEFINED: JsValue = JsValue::_new(JSIDX_UNDEFINED);

    /// The `true` JS value constant.
    pub const TRUE: JsValue = JsValue::_new(JSIDX_TRUE);

    /// The `false` JS value constant.
    pub const FALSE: JsValue = JsValue::_new(JSIDX_FALSE);

    /// Create a new JsValue from an index (const fn for static values).
    #[inline]
    const fn _new(idx: u64) -> JsValue {
        JsValue { idx }
    }

    /// Create a new JsValue from a heap ID.
    ///
    /// This is called internally when decoding a value from JS.
    pub(crate) fn from_id(id: u64) -> Self {
        Self { idx: id }
    }

    /// Get the heap ID for this value.
    ///
    /// This is used internally for encoding values to send to JS.
    pub(crate) fn id(&self) -> u64 {
        self.idx
    }

    /// Creates a new JS value representing `undefined`.
    #[inline]
    pub const fn undefined() -> JsValue {
        JsValue::UNDEFINED
    }

    /// Creates a new JS value representing `null`.
    #[inline]
    pub const fn null() -> JsValue {
        JsValue::NULL
    }

    /// Creates a new JS value which is a boolean.
    #[inline]
    pub const fn from_bool(b: bool) -> JsValue {
        if b {
            JsValue::TRUE
        } else {
            JsValue::FALSE
        }
    }
}

impl Clone for JsValue {
    #[inline]
    fn clone(&self) -> JsValue {
        // Reserved values don't need cloning - they're constants
        if self.idx < JSIDX_RESERVED {
            return JsValue { idx: self.idx };
        }

        // Clone the value on the JS heap
        let clone_fn: JSFunction<fn(u64) -> JsValue> = JSFunction::new(CLONE_HEAP_REF_FN_ID);
        clone_fn.call(self.idx)
    }
}

impl Drop for JsValue {
    #[inline]
    fn drop(&mut self) {
        // Reserved values don't need dropping - they're constants
        if self.idx < JSIDX_RESERVED {
            return;
        }

        // Drop the value on the JS heap
        crate::batch::queue_js_drop(self.idx);
    }
}

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsValue")
            .field("idx", &self.idx)
            .finish()
    }
}

impl PartialEq for JsValue {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl Eq for JsValue {}

impl std::hash::Hash for JsValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}

impl Default for JsValue {
    fn default() -> Self {
        Self::UNDEFINED
    }
}
