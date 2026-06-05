//! JsValue - An opaque reference to a JavaScript value
//!
//! This type represents a reference to a JavaScript value on the JS heap.
//! API compatible with wasm-bindgen's JsValue.

use crate::__rt::{JsRef, JsRefEncode};
use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt;
use core::ptr::NonNull;
use wry_bindgen_core::Clamped;

#[inline]
fn is_special_value_id(id: JsRef) -> bool {
    id.is_special_value()
}

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
    // A JsRef is only meaningful inside the thread-local runtime that created
    // it; using one elsewhere is a runtime error.
    idx: JsRef,
}

impl JsValue {
    /// The `null` JS value constant.
    pub const NULL: JsValue = JsValue::from_ref(JsRef::NULL);

    /// The `undefined` JS value constant.
    pub const UNDEFINED: JsValue = JsValue::from_ref(JsRef::UNDEFINED);

    /// The `true` JS value constant.
    pub const TRUE: JsValue = JsValue::from_ref(JsRef::TRUE);

    /// The `false` JS value constant.
    pub const FALSE: JsValue = JsValue::from_ref(JsRef::FALSE);

    /// Create a new JsValue from a JS heap reference.
    ///
    /// This is called internally when decoding or reserving a value from JS.
    #[inline]
    pub(crate) const fn from_ref(js_ref: JsRef) -> Self {
        Self { idx: js_ref }
    }

    /// Get the JS heap reference for this value.
    ///
    /// This is used internally for encoding values to send to JS.
    #[inline]
    pub(crate) fn js_ref(&self) -> JsRef {
        self.idx
    }

    /// Serializes a Rust value into a `JsValue` by serializing to JSON and
    /// invoking `JSON.parse` on the JS side.
    ///
    /// **This function is deprecated**, mirroring upstream wasm-bindgen. Use
    /// [`serde-wasm-bindgen`](https://docs.rs/serde-wasm-bindgen) instead.
    ///
    /// Usage requires activating the `serde-serialize` feature.
    ///
    /// # Errors
    ///
    /// Returns any error encountered when serializing `T` into JSON.
    #[cfg(feature = "serde-serialize")]
    #[deprecated = "causes dependency cycles, use `serde-wasm-bindgen` instead"]
    pub fn from_serde<T>(t: &T) -> serde_json::Result<JsValue>
    where
        T: serde::ser::Serialize + ?Sized,
    {
        let json = serde_json::to_string(t)?;
        Ok(crate::__wry_call_js_function!(
            "(s) => JSON.parse(s)",
            fn(&str) -> JsValue,
            (json.as_str())
        ))
    }

    /// Invokes `JSON.stringify` on this value and parses the resulting JSON into
    /// an arbitrary Rust value.
    ///
    /// **This function is deprecated**, mirroring upstream wasm-bindgen. Use
    /// [`serde-wasm-bindgen`](https://docs.rs/serde-wasm-bindgen) instead.
    ///
    /// Usage requires activating the `serde-serialize` feature.
    ///
    /// # Errors
    ///
    /// Returns any error encountered when parsing the JSON into a `T`.
    #[cfg(feature = "serde-serialize")]
    #[deprecated = "causes dependency cycles, use `serde-wasm-bindgen` instead"]
    pub fn into_serde<T>(&self) -> serde_json::Result<T>
    where
        T: for<'a> serde::de::Deserialize<'a>,
    {
        // `JSON.stringify(undefined) === undefined`; reinterpret that as JSON
        // `null` so it round-trips, matching upstream's behavior.
        let json: String = crate::__wry_call_js_function!(
            "(v) => JSON.stringify(v) ?? \"null\"",
            fn(JsValue) -> String,
            (self.clone())
        );
        serde_json::from_str(&json)
    }

    /// Returns the value as f64 without type checking.
    /// Used by serde-wasm-bindgen for numeric conversions.
    #[inline]
    pub fn unchecked_into_f64(&self) -> f64 {
        // Unary `+` coercion, matching wasm-bindgen (so e.g. the string "5" becomes 5).
        crate::js_helpers::js_as_number(self)
    }

    /// Check if this value is an instance of a specific JS type.
    #[inline]
    pub fn has_type<T: crate::JsCast>(&self) -> bool {
        T::is_type_of(self)
    }

    /// Get the internal ABI representation (heap index), consuming self.
    /// This is used by the convert module for low-level interop.
    /// Returns u32 for wasm-bindgen compatibility.
    #[inline]
    pub fn into_abi(self) -> u32 {
        let id = self.idx.into_abi();
        core::mem::forget(self);
        id
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
        if b { JsValue::TRUE } else { JsValue::FALSE }
    }

    /// Creates a JS string from a Rust string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> JsValue {
        s.into()
    }

    /// Creates a JS number from an f64.
    pub fn from_f64(n: f64) -> JsValue {
        n.into()
    }

    /// Creates a JS value which is a bigint from a string representing a number.
    pub fn bigint_from_str(s: &str) -> JsValue {
        crate::js_helpers::js_bigint_from_str(s)
    }

    /// Creates a new JS symbol with the optional description specified.
    pub fn symbol(description: Option<&str>) -> JsValue {
        crate::js_helpers::js_symbol_new(description)
    }
}

impl Clone for JsValue {
    #[inline]
    fn clone(&self) -> JsValue {
        // Special constants don't need cloning. Borrow-stack IDs are below
        // JSIDX_OFFSET and must be promoted to owned heap refs when cloned.
        if is_special_value_id(self.idx) {
            return JsValue::from_ref(self.idx);
        }

        // Clone the value on the JS heap
        crate::js_helpers::js_clone_heap_ref(self)
    }
}

impl JsRefEncode for JsValue {
    #[inline]
    fn js_ref(&self) -> JsRef {
        self.js_ref()
    }
}

impl Drop for JsValue {
    #[inline]
    fn drop(&mut self) {
        // Borrowed refs and special constants don't own JS heap slots.
        if !self.idx.is_owned_heap_ref() {
            return;
        }

        // Drop the value on the JS heap
        self.idx.drop_js_object();
    }
}

impl<'a> PartialEq<&'a str> for JsValue {
    fn eq(&self, other: &&'a str) -> bool {
        match self.as_string() {
            Some(s) => &s == other,
            None => false,
        }
    }
}

impl PartialEq<JsValue> for &str {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_string() {
            Some(s) => self == &s,
            None => false,
        }
    }
}

impl PartialEq<str> for JsValue {
    fn eq(&self, other: &str) -> bool {
        match self.as_string() {
            Some(s) => s == other,
            None => false,
        }
    }
}

impl PartialEq<String> for JsValue {
    fn eq(&self, other: &String) -> bool {
        match self.as_string() {
            Some(s) => &s == other,
            None => false,
        }
    }
}

impl PartialEq<JsValue> for String {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_string() {
            Some(s) => self == &s,
            None => false,
        }
    }
}

impl<'a> PartialEq<&'a String> for JsValue {
    fn eq(&self, other: &&'a String) -> bool {
        match self.as_string() {
            Some(s) => &s == *other,
            None => false,
        }
    }
}

impl PartialEq<JsValue> for &String {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_string() {
            Some(s) => *self == &s,
            None => false,
        }
    }
}

impl PartialEq<bool> for JsValue {
    fn eq(&self, other: &bool) -> bool {
        match self.as_bool() {
            Some(b) => b == *other,
            None => false,
        }
    }
}

impl PartialEq<JsValue> for bool {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_bool() {
            Some(b) => *self == b,
            None => false,
        }
    }
}

impl PartialEq<f32> for JsValue {
    fn eq(&self, other: &f32) -> bool {
        match self.as_f64() {
            Some(n) => n == (*other as f64),
            None => false,
        }
    }
}

impl PartialEq<JsValue> for f32 {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_f64() {
            Some(n) => (*self as f64) == n,
            None => false,
        }
    }
}

impl PartialEq<f64> for JsValue {
    fn eq(&self, other: &f64) -> bool {
        match self.as_f64() {
            Some(n) => n == *other,
            None => false,
        }
    }
}

impl PartialEq<JsValue> for f64 {
    fn eq(&self, other: &JsValue) -> bool {
        match other.as_f64() {
            Some(n) => *self == n,
            None => false,
        }
    }
}

// Macro for integer PartialEq implementations
macro_rules! impl_partial_eq_int {
    ($($t:ty),*) => {
        $(
            impl PartialEq<$t> for JsValue {
                fn eq(&self, other: &$t) -> bool {
                    match self.as_f64() {
                        Some(n) => n == (*other as f64),
                        None => false,
                    }
                }
            }

            impl PartialEq<JsValue> for $t {
                fn eq(&self, other: &JsValue) -> bool {
                    match other.as_f64() {
                        Some(n) => (*self as f64) == n,
                        None => false,
                    }
                }
            }
        )*
    };
}

impl_partial_eq_int!(i8, i16, i32, isize, u8, u16, u32, usize);

// 64/128-bit integers are JS `BigInt`s, so they compare via bigint `===` rather
// than `as_f64` (which is `None` for a bigint). Matches wasm-bindgen.
macro_rules! impl_partial_eq_bigint {
    ($($t:ty),*) => {
        $(
            impl PartialEq<$t> for JsValue {
                fn eq(&self, other: &$t) -> bool {
                    *self == JsValue::from(*other)
                }
            }

            impl PartialEq<JsValue> for $t {
                fn eq(&self, other: &JsValue) -> bool {
                    JsValue::from(*self) == *other
                }
            }
        )*
    };
}

impl_partial_eq_bigint!(i64, u64, i128, u128);

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JsValue({})", self.as_debug_string())
    }
}

impl PartialEq for JsValue {
    /// Compares two `JsValue`s with JS `===`, matching wasm-bindgen.
    fn eq(&self, other: &Self) -> bool {
        crate::js_helpers::js_strict_eq(self, other)
    }
}

impl Eq for JsValue {}

impl core::hash::Hash for JsValue {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&self.idx, state);
    }
}

impl Default for JsValue {
    fn default() -> Self {
        Self::UNDEFINED
    }
}

// Additional methods needed by js-sys for BigInt operations
impl JsValue {
    /// Checked division.
    pub fn checked_div(&self, rhs: &Self) -> Self {
        crate::js_helpers::js_checked_div(self, rhs)
    }

    /// Power operation.
    pub fn pow(&self, rhs: &Self) -> Self {
        crate::js_helpers::js_pow(self, rhs)
    }

    /// Bitwise AND.
    pub fn bit_and(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_bit_and(self, rhs)
    }

    /// Bitwise OR.
    pub fn bit_or(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_bit_or(self, rhs)
    }

    /// Bitwise XOR.
    pub fn bit_xor(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_bit_xor(self, rhs)
    }

    /// Bitwise NOT.
    pub fn bit_not(&self) -> JsValue {
        crate::js_helpers::js_bit_not(self)
    }

    /// Left shift.
    pub fn shl(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_shl(self, rhs)
    }

    /// Signed right shift.
    pub fn shr(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_shr(self, rhs)
    }

    /// Unsigned right shift.
    pub fn unsigned_shr(&self, rhs: &Self) -> u32 {
        crate::js_helpers::js_unsigned_shr(self, rhs)
    }

    /// Add.
    pub fn add(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_add(self, rhs)
    }

    /// Subtract.
    pub fn sub(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_sub(self, rhs)
    }

    /// Multiply.
    pub fn mul(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_mul(self, rhs)
    }

    /// Divide.
    pub fn div(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_div(self, rhs)
    }

    /// Remainder.
    pub fn rem(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_rem(self, rhs)
    }

    /// Negate.
    pub fn neg(&self) -> JsValue {
        crate::js_helpers::js_neg(self)
    }

    /// Less than comparison.
    pub fn lt(&self, other: &Self) -> bool {
        crate::js_helpers::js_lt(self, other)
    }

    /// Less than or equal comparison.
    pub fn le(&self, other: &Self) -> bool {
        crate::js_helpers::js_le(self, other)
    }

    /// Greater than comparison.
    pub fn gt(&self, other: &Self) -> bool {
        crate::js_helpers::js_gt(self, other)
    }

    /// Greater than or equal comparison.
    pub fn ge(&self, other: &Self) -> bool {
        crate::js_helpers::js_ge(self, other)
    }

    /// Loose equality (==).
    pub fn loose_eq(&self, other: &Self) -> bool {
        crate::js_helpers::js_loose_eq(self, other)
    }

    /// Check if this value is a falsy value in JavaScript.
    pub fn is_falsy(&self) -> bool {
        crate::js_helpers::js_is_falsy(self)
    }

    /// Check if this value is a truthy value in JavaScript.
    pub fn is_truthy(&self) -> bool {
        crate::js_helpers::js_is_truthy(self)
    }

    /// Check if this value is an object.
    pub fn is_object(&self) -> bool {
        crate::js_helpers::js_is_object(self)
    }

    /// Check if this value is a function.
    pub fn is_function(&self) -> bool {
        crate::js_helpers::js_is_function(self)
    }

    /// Check if this value is a string.
    pub fn is_string(&self) -> bool {
        crate::js_helpers::js_is_string(self)
    }

    /// Check if this value is a symbol.
    pub fn is_symbol(&self) -> bool {
        crate::js_helpers::js_is_symbol(self)
    }

    /// Check if this value is a bigint.
    pub fn is_bigint(&self) -> bool {
        crate::js_helpers::js_is_bigint(self)
    }

    /// Check if this value is an Array.
    pub fn is_array(&self) -> bool {
        crate::js_helpers::js_is_array(self)
    }

    /// Check if this value is undefined.
    pub fn is_undefined(&self) -> bool {
        if self.idx == JsRef::UNDEFINED {
            return true;
        }
        crate::js_helpers::js_is_undefined(self)
    }

    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        if self.idx == JsRef::NULL {
            return true;
        }
        crate::js_helpers::js_is_null(self)
    }

    /// Check if this value is null or undefined.
    pub fn is_null_or_undefined(&self) -> bool {
        if self.idx == JsRef::NULL || self.idx == JsRef::UNDEFINED {
            return true;
        }
        crate::js_helpers::js_is_null_or_undefined(self)
    }

    /// Get the typeof this value as a string.
    pub fn js_typeof(&self) -> JsValue {
        crate::js_helpers::js_typeof(self)
    }

    /// Check if this value has a property with the given name.
    pub fn js_in(&self, obj: &JsValue) -> bool {
        crate::js_helpers::js_in(self, obj)
    }

    /// Get the value as a bool.
    pub fn as_bool(&self) -> Option<bool> {
        if self.idx == JsRef::TRUE {
            Some(true)
        } else if self.idx == JsRef::FALSE {
            Some(false)
        } else if self.idx == JsRef::UNDEFINED || self.idx == JsRef::NULL {
            None
        } else if crate::js_helpers::js_is_true(self) {
            Some(true)
        } else if crate::js_helpers::js_is_false(self) {
            Some(false)
        } else {
            None
        }
    }

    /// Get the value as an f64.
    pub fn as_f64(&self) -> Option<f64> {
        crate::js_helpers::js_as_f64(self)
    }

    /// Get the value as a string.
    pub fn as_string(&self) -> Option<String> {
        crate::js_helpers::js_as_string(self)
    }

    /// Get a debug string representation of the value.
    pub fn as_debug_string(&self) -> String {
        crate::js_helpers::js_debug_string(self)
    }
}

// Operator trait implementations for JsValue references
use core::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Neg, Not, Rem, Shl, Shr, Sub};

impl Neg for &JsValue {
    type Output = JsValue;
    fn neg(self) -> Self::Output {
        JsValue::neg(self)
    }
}

impl Not for &JsValue {
    type Output = bool;

    fn not(self) -> Self::Output {
        JsValue::is_falsy(self)
    }
}

impl BitAnd for &JsValue {
    type Output = JsValue;
    fn bitand(self, rhs: Self) -> Self::Output {
        JsValue::bit_and(self, rhs)
    }
}

impl BitOr for &JsValue {
    type Output = JsValue;
    fn bitor(self, rhs: Self) -> Self::Output {
        JsValue::bit_or(self, rhs)
    }
}

impl BitXor for &JsValue {
    type Output = JsValue;
    fn bitxor(self, rhs: Self) -> Self::Output {
        JsValue::bit_xor(self, rhs)
    }
}

impl Shl for &JsValue {
    type Output = JsValue;
    fn shl(self, rhs: Self) -> Self::Output {
        JsValue::shl(self, rhs)
    }
}

impl Shr for &JsValue {
    type Output = JsValue;
    fn shr(self, rhs: Self) -> Self::Output {
        JsValue::shr(self, rhs)
    }
}

impl Add for &JsValue {
    type Output = JsValue;
    fn add(self, rhs: Self) -> Self::Output {
        JsValue::add(self, rhs)
    }
}

impl Sub for &JsValue {
    type Output = JsValue;
    fn sub(self, rhs: Self) -> Self::Output {
        JsValue::sub(self, rhs)
    }
}

impl Mul for &JsValue {
    type Output = JsValue;
    fn mul(self, rhs: Self) -> Self::Output {
        JsValue::mul(self, rhs)
    }
}

impl Div for &JsValue {
    type Output = JsValue;
    fn div(self, rhs: Self) -> Self::Output {
        JsValue::div(self, rhs)
    }
}

impl Rem for &JsValue {
    type Output = JsValue;
    fn rem(self, rhs: Self) -> Self::Output {
        JsValue::rem(self, rhs)
    }
}

impl Neg for JsValue {
    type Output = JsValue;
    fn neg(self) -> JsValue {
        JsValue::neg(&self)
    }
}

impl Not for JsValue {
    type Output = bool;
    fn not(self) -> Self::Output {
        JsValue::is_falsy(&self)
    }
}

// Macro for binary operators with all ownership combinations
macro_rules! impl_binary_op {
    ($trait:ident, $method:ident, $js_method:ident) => {
        // JsValue op JsValue
        impl $trait for JsValue {
            type Output = JsValue;
            fn $method(self, rhs: JsValue) -> JsValue {
                JsValue::$js_method(&self, &rhs)
            }
        }

        // JsValue op &JsValue
        impl $trait<&JsValue> for JsValue {
            type Output = JsValue;
            fn $method(self, rhs: &JsValue) -> JsValue {
                JsValue::$js_method(&self, rhs)
            }
        }

        // &JsValue op JsValue
        impl<'a> $trait<JsValue> for &'a JsValue {
            type Output = JsValue;
            fn $method(self, rhs: JsValue) -> JsValue {
                JsValue::$js_method(self, &rhs)
            }
        }
    };
}

impl_binary_op!(Add, add, add);
impl_binary_op!(Sub, sub, sub);
impl_binary_op!(Mul, mul, mul);
impl_binary_op!(Div, div, div);
impl_binary_op!(Rem, rem, rem);
impl_binary_op!(BitAnd, bitand, bit_and);
impl_binary_op!(BitOr, bitor, bit_or);
impl_binary_op!(BitXor, bitxor, bit_xor);
impl_binary_op!(Shl, shl, shl);
impl_binary_op!(Shr, shr, shr);

impl From<bool> for JsValue {
    fn from(s: bool) -> JsValue {
        JsValue::from_bool(s)
    }
}

impl<T> From<*mut T> for JsValue {
    fn from(s: *mut T) -> JsValue {
        JsValue::from(s as usize)
    }
}

impl<T> From<*const T> for JsValue {
    fn from(s: *const T) -> JsValue {
        JsValue::from(s as usize)
    }
}

impl<T> From<NonNull<T>> for JsValue {
    fn from(s: NonNull<T>) -> JsValue {
        JsValue::from(s.as_ptr() as usize)
    }
}

impl<T> From<Vec<T>> for JsValue
where
    Vec<T>: crate::__rt::BinaryEncode + crate::__rt::EncodeTypeDef,
{
    fn from(vector: Vec<T>) -> Self {
        crate::__rt::wbg_cast(vector)
    }
}

impl<T> From<Box<[T]>> for JsValue
where
    Box<[T]>: crate::__rt::BinaryEncode + crate::__rt::EncodeTypeDef,
{
    fn from(vector: Box<[T]>) -> Self {
        crate::__rt::wbg_cast(vector)
    }
}

impl<T> From<Clamped<Vec<T>>> for JsValue
where
    Clamped<Vec<T>>: crate::__rt::BinaryEncode + crate::__rt::EncodeTypeDef,
{
    fn from(vector: Clamped<Vec<T>>) -> Self {
        crate::__rt::wbg_cast(vector)
    }
}

impl<T> From<Clamped<Box<[T]>>> for JsValue
where
    Clamped<Vec<T>>: crate::__rt::BinaryEncode + crate::__rt::EncodeTypeDef,
{
    fn from(vector: Clamped<Box<[T]>>) -> Self {
        crate::__rt::wbg_cast(Clamped(vector.0.into_vec()))
    }
}
