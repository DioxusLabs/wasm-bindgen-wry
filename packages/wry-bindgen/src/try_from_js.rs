//! Conversions between Rust values and `JsValue`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{JsCast, JsValue, convert};

macro_rules! cast {
    (($from:ty => $to:ty) $val:expr) => {{ $crate::__rt::wbg_cast::<$from, $to>($val) }};
}

macro_rules! to_js_value {
    ($ty:ty) => {
        impl From<$ty> for $crate::JsValue {
            fn from(val: $ty) -> Self {
                cast! {($ty => $crate::JsValue) val}
            }
        }
    };
}

macro_rules! from_js_value {
    ($ty:ty) => {
        impl From<$crate::JsValue> for $ty {
            fn from(val: $crate::JsValue) -> Self {
                cast! {($crate::JsValue => $ty) val}
            }
        }
    };
}

impl TryFrom<JsValue> for u64 {
    type Error = JsValue;

    fn try_from(v: JsValue) -> Result<Self, JsValue> {
        <Self as convert::TryFromJsValue>::try_from_js_value(v)
    }
}

impl TryFrom<JsValue> for i64 {
    type Error = JsValue;

    fn try_from(v: JsValue) -> Result<Self, JsValue> {
        <Self as convert::TryFromJsValue>::try_from_js_value(v)
    }
}

impl TryFrom<JsValue> for f64 {
    type Error = JsValue;

    fn try_from(val: JsValue) -> Result<Self, Self::Error> {
        val.as_f64().ok_or(val)
    }
}

impl TryFrom<&JsValue> for f64 {
    type Error = JsValue;

    fn try_from(val: &JsValue) -> Result<Self, Self::Error> {
        val.as_f64().ok_or_else(|| val.clone())
    }
}

impl TryFrom<JsValue> for i128 {
    type Error = JsValue;

    fn try_from(v: JsValue) -> Result<Self, JsValue> {
        <Self as convert::TryFromJsValue>::try_from_js_value(v)
    }
}

impl TryFrom<JsValue> for u128 {
    type Error = JsValue;

    fn try_from(v: JsValue) -> Result<Self, JsValue> {
        <Self as convert::TryFromJsValue>::try_from_js_value(v)
    }
}

impl TryFrom<JsValue> for String {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        value.as_string().ok_or(value)
    }
}

impl convert::TryFromJsValue for String {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        value.as_string()
    }
}

impl convert::TryFromJsValue for bool {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        value.as_bool()
    }
}

impl convert::TryFromJsValue for char {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        let s = value.as_string()?;
        let mut chars = s.chars();
        let c = chars.next()?;
        if chars.next().is_none() {
            Some(c)
        } else {
            None
        }
    }
}

impl convert::TryFromJsValue for () {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        if value.is_undefined() { Some(()) } else { None }
    }
}

impl<T: convert::TryFromJsValue> convert::TryFromJsValue for Option<T> {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        if value.is_undefined() {
            Some(None)
        } else {
            T::try_from_js_value_ref(value).map(Some)
        }
    }
}

impl<T: convert::TryFromJsValue> convert::TryFromJsValue for Vec<T> {
    fn try_from_js_value_ref(value: &JsValue) -> Option<Self> {
        if !value.is_array() {
            return None;
        }
        let length = crate::js_helpers::js_reflect_get(value, &JsValue::from_str("length"));
        let len = length.as_f64()? as u32;
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len {
            let element = crate::js_helpers::js_reflect_get(value, &JsValue::from_f64(i as f64));
            out.push(T::try_from_js_value(element).ok()?);
        }
        Some(out)
    }
}

fn js_number_is_integer_in_range(number: f64, min: f64, max: f64) -> bool {
    number.is_finite() && number.fract() == 0.0 && (min..=max).contains(&number)
}

macro_rules! try_from_js_value_signed_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl convert::TryFromJsValue for $ty {
                fn try_from_js_value_ref(val: &JsValue) -> Option<$ty> {
                    let number = val.as_f64()?;
                    if js_number_is_integer_in_range(number, <$ty>::MIN as f64, <$ty>::MAX as f64) {
                        Some(number as $ty)
                    } else {
                        None
                    }
                }
            }
        )*
    };
}

macro_rules! try_from_js_value_unsigned_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl convert::TryFromJsValue for $ty {
                fn try_from_js_value_ref(val: &JsValue) -> Option<$ty> {
                    let number = val.as_f64()?;
                    if js_number_is_integer_in_range(number, 0.0, <$ty>::MAX as f64) {
                        Some(number as $ty)
                    } else {
                        None
                    }
                }
            }
        )*
    };
}

try_from_js_value_signed_int!(i8, i16, i32);
try_from_js_value_unsigned_int!(u8, u16, u32);

impl convert::TryFromJsValue for f32 {
    fn try_from_js_value_ref(val: &JsValue) -> Option<f32> {
        val.as_f64().map(|n| n as f32)
    }
}

impl convert::TryFromJsValue for f64 {
    fn try_from_js_value_ref(val: &JsValue) -> Option<f64> {
        val.as_f64()
    }
}

impl convert::TryFromJsValue for i64 {
    fn try_from_js_value_ref(val: &JsValue) -> Option<i64> {
        crate::js_helpers::js_bigint_get_as_i64(val)
    }
}

impl convert::TryFromJsValue for u64 {
    fn try_from_js_value_ref(val: &JsValue) -> Option<u64> {
        crate::js_helpers::js_bigint_to_string(val)?.parse().ok()
    }
}

impl convert::TryFromJsValue for i128 {
    fn try_from_js_value_ref(v: &JsValue) -> Option<i128> {
        crate::js_helpers::js_bigint_to_string(v)?.parse().ok()
    }
}

impl convert::TryFromJsValue for u128 {
    fn try_from_js_value_ref(v: &JsValue) -> Option<u128> {
        crate::js_helpers::js_bigint_to_string(v)?.parse().ok()
    }
}

impl convert::TryFromJsValue for isize {
    fn try_from_js_value_ref(val: &JsValue) -> Option<isize> {
        val.as_f64().map(|n| n as isize)
    }
}

impl convert::TryFromJsValue for usize {
    fn try_from_js_value_ref(val: &JsValue) -> Option<usize> {
        val.as_f64().map(|n| n as usize)
    }
}

to_js_value!(i8);
from_js_value!(i8);
to_js_value!(i16);
from_js_value!(i16);
to_js_value!(i32);
from_js_value!(i32);
to_js_value!(i64);
to_js_value!(i128);
to_js_value!(u8);
from_js_value!(u8);
to_js_value!(u16);
from_js_value!(u16);
to_js_value!(u32);
from_js_value!(u32);
to_js_value!(u64);
to_js_value!(u128);
to_js_value!(f32);
from_js_value!(f32);
to_js_value!(f64);
to_js_value!(usize);
from_js_value!(usize);
to_js_value!(isize);
from_js_value!(isize);
impl<'a> From<&'a str> for JsValue {
    fn from(s: &'a str) -> JsValue {
        cast! {(String => JsValue) s.to_string()}
    }
}
impl<'a> From<&'a String> for JsValue {
    fn from(s: &'a String) -> JsValue {
        cast! {(String => JsValue) s.clone()}
    }
}
impl<'a, T> From<&'a T> for JsValue
where
    T: JsCast,
{
    fn from(s: &'a T) -> JsValue {
        s.as_ref().clone()
    }
}
impl From<String> for JsValue {
    fn from(s: String) -> JsValue {
        cast! {(String => JsValue) s}
    }
}
to_js_value!(());
from_js_value!(());

impl<T> From<Option<T>> for JsValue
where
    T: Into<JsValue>,
{
    fn from(s: Option<T>) -> JsValue {
        match s {
            Some(s) => s.into(),
            None => JsValue::undefined(),
        }
    }
}
