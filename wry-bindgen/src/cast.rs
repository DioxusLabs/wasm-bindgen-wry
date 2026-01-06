//! JsCast - Type casting trait for JavaScript types
//!
//! This trait provides methods for casting between JavaScript types,
//! similar to wasm-bindgen's JsCast trait.

use crate::JsValue;

/// Trait for types that can be cast to and from JsValue.
///
/// This is the wry-bindgen equivalent of wasm-bindgen's `JsCast` trait.
/// It enables safe and unsafe casting between JavaScript types.
pub trait JsCast: AsRef<JsValue> + Into<JsValue> + Sized {
    /// Check if a JsValue is an instance of this type.
    ///
    /// This performs a runtime instanceof check in JavaScript.
    fn instanceof(val: &JsValue) -> bool;

    /// Performs a dynamic type check to see whether the `JsValue` provided
    /// is a value of this type.
    ///
    /// Unlike `instanceof`, this can be specialized to check for primitive types
    /// or perform other type checks that aren't possible with instanceof.
    /// The default implementation falls back to `instanceof`.
    #[inline]
    fn is_type_of(val: &JsValue) -> bool {
        Self::instanceof(val)
    }

    /// Test whether this JS value has a type `T`.
    ///
    /// This method will dynamically check to see if this JS object can be
    /// casted to the JS object of type `T`. Usually this uses the `instanceof`
    /// operator, but can be customized with `is_type_of`. This also works
    /// with primitive types like booleans/strings/numbers as well as cross-realm
    /// objects like `Array` which can originate from other iframes.
    ///
    /// In general this is intended to be a more robust version of
    /// `is_instance_of`, but if you want strictly the `instanceof` operator
    /// it's recommended to use that instead.
    #[inline]
    fn has_type<T: JsCast>(&self) -> bool {
        T::is_type_of(self.as_ref())
    }

    /// Unchecked cast from JsValue to this type.
    ///
    /// # Safety
    /// This does not perform any runtime checks. The caller must ensure
    /// the value is actually of the correct type.
    fn unchecked_from_js(val: JsValue) -> Self;

    /// Unchecked cast from a JsValue reference to a reference of this type.
    ///
    /// # Safety
    /// This does not perform any runtime checks. The caller must ensure
    /// the value is actually of the correct type.
    fn unchecked_from_js_ref(val: &JsValue) -> &Self;

    /// Try to cast this value to type T.
    ///
    /// Returns `Ok(T)` if the value is an instance of T,
    /// otherwise returns `Err(self)` with the original value.
    fn dyn_into<T: JsCast>(self) -> Result<T, Self> {
        if T::is_type_of(self.as_ref()) {
            Ok(T::unchecked_from_js(self.into()))
        } else {
            Err(self)
        }
    }

    /// Try to get a reference to type T from this value.
    ///
    /// Returns `Some(&T)` if the value is an instance of T,
    /// otherwise returns `None`.
    fn dyn_ref<T: JsCast>(&self) -> Option<&T> {
        if T::is_type_of(self.as_ref()) {
            Some(T::unchecked_from_js_ref(self.as_ref()))
        } else {
            None
        }
    }

    /// Test whether this JS value is an instance of the type `T`.
    ///
    /// This method performs a dynamic check (at runtime) using the JS
    /// `instanceof` operator. This method returns `self instanceof T`.
    ///
    /// Note that `instanceof` does not always work with primitive values or
    /// across different realms (e.g. iframes). If you're not sure whether you
    /// specifically need only `instanceof` it's recommended to use `has_type`
    /// instead.
    fn is_instance_of<T: JsCast>(&self) -> bool {
        T::instanceof(self.as_ref())
    }

    /// Unchecked cast to another type.
    fn unchecked_into<T: JsCast>(self) -> T {
        T::unchecked_from_js(self.into())
    }

    /// Unchecked cast to a reference of another type.
    fn unchecked_ref<T: JsCast>(&self) -> &T {
        T::unchecked_from_js_ref(self.as_ref())
    }
}

/// Implement JsCast for JsValue itself (identity cast)
impl JsCast for JsValue {
    fn instanceof(_val: &JsValue) -> bool {
        true // Everything is a JsValue
    }

    fn unchecked_from_js(val: JsValue) -> Self {
        val
    }

    fn unchecked_from_js_ref(val: &JsValue) -> &Self {
        val
    }
}

impl AsRef<JsValue> for JsValue {
    fn as_ref(&self) -> &JsValue {
        self
    }
}
