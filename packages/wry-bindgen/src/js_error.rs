//! JavaScript `Error` wrapper.

use crate::{__rt, IntoJsGeneric, JsCast, JsValue, convert};

/// A JavaScript Error object.
///
/// This type is used to create JavaScript Error objects that can be thrown or returned.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct JsError {
    value: JsValue,
}

impl JsError {
    /// Create a new JavaScript Error with the given message.
    pub fn new(s: &str) -> JsError {
        JsError {
            value: crate::__wry_call_js_function!(
                "(msg) => new Error(msg)",
                fn(&str) -> JsValue,
                (s)
            ),
        }
    }
}

impl From<JsError> for JsValue {
    fn from(error: JsError) -> Self {
        error.value
    }
}

impl From<JsValue> for JsError {
    fn from(value: JsValue) -> Self {
        JsError { value }
    }
}

// Matches wasm-bindgen: `JsError` implements `From<E> where E: std::error::Error`,
// which is what enables `?` on `Result<_, JsError>`. For this blanket impl to avoid
// colliding with core's reflexive `From<T> for T`, `JsError` itself must NOT implement
// `std::error::Error` (upstream deliberately does not implement it either).
#[cfg(feature = "std")]
impl<E> From<E> for JsError
where
    E: std::error::Error,
{
    fn from(error: E) -> Self {
        use alloc::string::ToString;

        JsError::new(&error.to_string())
    }
}

impl AsRef<JsValue> for JsError {
    fn as_ref(&self) -> &JsValue {
        &self.value
    }
}

impl core::fmt::Display for JsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "JsError")
    }
}

impl JsCast for JsError {
    fn instanceof(val: &JsValue) -> bool {
        crate::js_helpers::js_is_error(val)
    }

    fn unchecked_from_js(val: JsValue) -> Self {
        JsError { value: val }
    }

    fn unchecked_from_js_ref(val: &JsValue) -> &Self {
        // SAFETY: #[repr(transparent)] guarantees same layout
        unsafe { &*(val as *const JsValue as *const JsError) }
    }
}

impl_js_value_wire!(for JsError, field value);

unsafe impl __rt::marker::ErasableGeneric for JsError {
    type Repr = JsValue;
}

impl IntoJsGeneric for JsError {
    type JsCanon = Self;

    fn to_js(self) -> Self {
        self
    }
}

impl convert::UpcastFrom<JsError> for JsError {}
impl convert::UpcastFrom<JsError> for JsValue {}
