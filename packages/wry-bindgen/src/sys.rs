//! JavaScript system value wrappers and promise compatibility traits.

use core::fmt;

use crate::{
    __rt::marker::ErasableGeneric, JsCast, JsError, JsGeneric, JsValue, convert::UpcastFrom,
};
use wry_bindgen_macro::wasm_bindgen;

/// Marker trait for values that are either a resolution value or a promise-like value.
pub trait Promising {
    type Resolution;
}

#[wasm_bindgen(wasm_bindgen = crate)]
extern "C" {
    /// The JavaScript `undefined` value.
    ///
    /// This type represents the JavaScript `undefined` primitive value and can be
    /// used as a generic type parameter to indicate that a value is `undefined`.
    #[wasm_bindgen(is_type_of = JsValue::is_undefined, typescript_type = "undefined", no_upcast)]
    #[derive(Clone, PartialEq)]
    pub type Undefined;
}

impl Undefined {
    /// The undefined constant.
    pub const UNDEFINED: Undefined = Self {
        obj: JsValue::UNDEFINED,
    };
}

impl Eq for Undefined {}

impl Default for Undefined {
    fn default() -> Self {
        Self::UNDEFINED
    }
}

impl fmt::Debug for Undefined {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("undefined")
    }
}

impl fmt::Display for Undefined {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("undefined")
    }
}

#[wasm_bindgen(wasm_bindgen = crate)]
extern "C" {
    /// The JavaScript `null` value.
    ///
    /// This type represents the JavaScript `null` primitive value and can be
    /// used as a generic type parameter to indicate that a value is `null`.
    #[wasm_bindgen(is_type_of = JsValue::is_null, typescript_type = "null", no_upcast)]
    #[derive(Clone, PartialEq)]
    pub type Null;
}

impl Null {
    /// The null constant.
    pub const NULL: Null = Self { obj: JsValue::NULL };
}

impl Eq for Null {}

impl Default for Null {
    fn default() -> Self {
        Self::NULL
    }
}

impl fmt::Debug for Null {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}

impl fmt::Display for Null {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}

impl UpcastFrom<Undefined> for Undefined {}
impl UpcastFrom<()> for Undefined {}
impl UpcastFrom<Undefined> for () {}
impl UpcastFrom<Undefined> for JsValue {}
impl UpcastFrom<Null> for Null {}
impl UpcastFrom<Null> for JsValue {}
impl UpcastFrom<()> for JsValue {}
impl UpcastFrom<()> for () {}

#[wasm_bindgen(wasm_bindgen = crate)]
extern "C" {
    /// A nullable JS value of type `T`.
    ///
    /// Unlike `Option<T>`, which is a Rust-side construct, `JsOption<T>` represents
    /// a JS value that may be `T`, `null`, or `undefined`, where the null status is
    /// not yet known in Rust. The value remains in JS until inspected via methods
    /// like [`is_empty`](Self::is_empty), [`as_option`](Self::as_option), or
    /// [`into_option`](Self::into_option).
    ///
    /// `T` must implement [`JsGeneric`], meaning it is any type that can be
    /// represented as a `JsValue` (e.g., `JsString`, `Number`, `Object`, etc.).
    /// `JsOption<T>` itself implements `JsGeneric`, so it can be used in all
    /// generic positions that accept JS types.
    #[wasm_bindgen(typescript_type = "any", no_upcast)]
    #[derive(Clone, PartialEq)]
    pub type JsOption<T = JsValue>;
}

impl<T: JsGeneric> JsOption<T> {
    #[inline]
    pub fn new() -> Self {
        Undefined::UNDEFINED.unchecked_into()
    }

    #[inline]
    pub fn wrap(val: T) -> Self {
        val.unchecked_into()
    }

    #[inline]
    pub fn from_option(opt: Option<T>) -> Self {
        match opt {
            Some(value) => Self::wrap(value),
            None => Self::new(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        AsRef::<JsValue>::as_ref(self).is_undefined()
    }

    #[inline]
    pub fn as_option(&self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(T::unchecked_from_js(AsRef::<JsValue>::as_ref(self).clone()))
        }
    }

    #[inline]
    pub fn into_option(self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(self.unchecked_into())
        }
    }

    #[inline]
    pub fn unwrap(self) -> T {
        self.expect("called `JsOption::unwrap()` on an empty value")
    }

    #[inline]
    pub fn expect(self, msg: &str) -> T {
        match self.into_option() {
            Some(value) => value,
            None => panic!("{}", msg),
        }
    }

    #[inline]
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        self.into_option().unwrap_or_default()
    }

    #[inline]
    pub fn unwrap_or_else<F>(self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.into_option().unwrap_or_else(f)
    }
}

impl<T: JsGeneric> Default for JsOption<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: JsGeneric + fmt::Debug> fmt::Debug for JsOption<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}?(", core::any::type_name::<T>())?;
        match self.as_option() {
            Some(value) => write!(f, "{value:?}")?,
            None => f.write_str("undefined")?,
        }
        f.write_str(")")
    }
}

impl<T: JsGeneric + fmt::Display> fmt::Display for JsOption<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}?(", core::any::type_name::<T>())?;
        match self.as_option() {
            Some(value) => write!(f, "{value}")?,
            None => f.write_str("undefined")?,
        }
        f.write_str(")")
    }
}

impl UpcastFrom<JsValue> for JsOption<JsValue> {}
impl<T> UpcastFrom<Undefined> for JsOption<T> {}
impl<T> UpcastFrom<()> for JsOption<T> {}
impl<T> UpcastFrom<JsOption<T>> for JsValue {}
impl<T, U> UpcastFrom<JsOption<U>> for JsOption<T> where T: UpcastFrom<U> {}

impl Promising for JsValue {
    type Resolution = JsValue;
}

impl Promising for () {
    type Resolution = Undefined;
}

macro_rules! promising_self {
        ($($ty:ty),* $(,)?) => {
            $(
                impl Promising for $ty {
                    type Resolution = $ty;
                }
            )*
        };
    }

promising_self!(
    bool, char, f32, f64, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, JsError
);

impl Promising for alloc::string::String {
    type Resolution = alloc::string::String;
}

impl<T: Promising> Promising for alloc::vec::Vec<T> {
    type Resolution = alloc::vec::Vec<T::Resolution>;
}

impl<T: Promising> Promising for Option<T> {
    type Resolution = Option<T::Resolution>;
}

impl<T: ErasableGeneric + Promising, E: ErasableGeneric> Promising for Result<T, E> {
    type Resolution = Result<T::Resolution, E>;
}
