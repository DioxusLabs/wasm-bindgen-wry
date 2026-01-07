//! JsValue - An opaque reference to a JavaScript value
//!
//! This type represents a reference to a JavaScript value on the JS heap.
//! API compatible with wasm-bindgen's JsValue.

use alloc::string::String;
use core::fmt;

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
    #[inline]
    pub(crate) fn from_id(id: u64) -> Self {
        Self { idx: id }
    }

    /// Get the heap ID for this value.
    ///
    /// This is used internally for encoding values to send to JS.
    #[inline]
    pub fn id(&self) -> u64 {
        self.idx
    }

    /// Returns the value as f64 without type checking.
    /// Used by serde-wasm-bindgen for numeric conversions.
    #[inline]
    pub fn unchecked_into_f64(&self) -> f64 {
        self.as_f64().unwrap_or(f64::NAN)
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
        let id = self.idx;
        core::mem::forget(self);
        id as u32
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
    pub fn from_str(s: &str) -> JsValue {
        s.try_into().unwrap()
    }

    /// Creates a JS number from an f64.
    pub fn from_f64(n: f64) -> JsValue {
        n.try_into().unwrap()
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
        crate::js_helpers::js_clone_heap_ref(self.idx)
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

impl PartialEq<&str> for JsValue {
    fn eq(&self, other: &&str) -> bool {
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

impl PartialEq<&String> for JsValue {
    fn eq(&self, other: &&String) -> bool {
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

impl_partial_eq_int!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_debug_string())
    }
}

impl PartialEq for JsValue {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl Eq for JsValue {}

impl core::hash::Hash for JsValue {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
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
    pub fn checked_div(&self, rhs: &JsValue) -> JsValue {
        crate::js_helpers::js_checked_div(self, rhs)
    }

    /// Power operation.
    pub fn pow(&self, rhs: &JsValue) -> JsValue {
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
    pub fn unsigned_shr(&self, rhs: &JsValue) -> u32 {
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
    pub fn lt(&self, rhs: &JsValue) -> bool {
        crate::js_helpers::js_lt(self, rhs)
    }

    /// Less than or equal comparison.
    pub fn le(&self, rhs: &JsValue) -> bool {
        crate::js_helpers::js_le(self, rhs)
    }

    /// Greater than comparison.
    pub fn gt(&self, rhs: &JsValue) -> bool {
        crate::js_helpers::js_gt(self, rhs)
    }

    /// Greater than or equal comparison.
    pub fn ge(&self, rhs: &JsValue) -> bool {
        crate::js_helpers::js_ge(self, rhs)
    }

    /// Loose equality (==).
    pub fn loose_eq(&self, rhs: &JsValue) -> bool {
        crate::js_helpers::js_loose_eq(self, rhs)
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

    /// Check if this value is undefined.
    pub fn is_undefined(&self) -> bool {
        if self.idx == JSIDX_UNDEFINED {
            return true;
        }
        crate::js_helpers::js_is_undefined(self)
    }

    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        if self.idx == JSIDX_NULL {
            return true;
        }
        crate::js_helpers::js_is_null(self)
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
        match self.idx {
            JSIDX_TRUE => Some(true),
            JSIDX_FALSE => Some(false),
            idx if idx < JSIDX_RESERVED => None,
            _ => {
                // For heap values, check via JS
                if crate::js_helpers::js_is_true(self) {
                    Some(true)
                } else if crate::js_helpers::js_is_false(self) {
                    Some(false)
                } else {
                    None
                }
            }
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
    fn neg(self) -> JsValue {
        JsValue::neg(self)
    }
}

impl Not for &JsValue {
    type Output = bool;

    fn not(self) -> Self::Output {
        JsValue::is_falsy(self)
    }
}

impl BitAnd<&JsValue> for &JsValue {
    type Output = JsValue;
    fn bitand(self, rhs: &JsValue) -> JsValue {
        JsValue::bit_and(self, rhs)
    }
}

impl BitOr<&JsValue> for &JsValue {
    type Output = JsValue;
    fn bitor(self, rhs: &JsValue) -> JsValue {
        JsValue::bit_or(self, rhs)
    }
}

impl BitXor<&JsValue> for &JsValue {
    type Output = JsValue;
    fn bitxor(self, rhs: &JsValue) -> JsValue {
        JsValue::bit_xor(self, rhs)
    }
}

impl Shl<&JsValue> for &JsValue {
    type Output = JsValue;
    fn shl(self, rhs: &JsValue) -> JsValue {
        JsValue::shl(self, rhs)
    }
}

impl Shr<&JsValue> for &JsValue {
    type Output = JsValue;
    fn shr(self, rhs: &JsValue) -> JsValue {
        JsValue::shr(self, rhs)
    }
}

impl Add<&JsValue> for &JsValue {
    type Output = JsValue;
    fn add(self, rhs: &JsValue) -> JsValue {
        JsValue::add(self, rhs)
    }
}

impl Sub<&JsValue> for &JsValue {
    type Output = JsValue;
    fn sub(self, rhs: &JsValue) -> JsValue {
        JsValue::sub(self, rhs)
    }
}

impl Mul<&JsValue> for &JsValue {
    type Output = JsValue;
    fn mul(self, rhs: &JsValue) -> JsValue {
        JsValue::mul(self, rhs)
    }
}

impl Div<&JsValue> for &JsValue {
    type Output = JsValue;
    fn div(self, rhs: &JsValue) -> JsValue {
        JsValue::div(self, rhs)
    }
}

impl Rem<&JsValue> for &JsValue {
    type Output = JsValue;
    fn rem(self, rhs: &JsValue) -> JsValue {
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
    fn from(val: bool) -> Self {
        JsValue::from_bool(val)
    }
}
