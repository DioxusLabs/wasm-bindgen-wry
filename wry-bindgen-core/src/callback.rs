//! Callback registration keys used by generated code.

use core::marker::PhantomData;

use wry_bindgen_abi::{BinaryEncode, EncodeTypeDef, EncodedData, ObjectHandle, TypeDef};

#[derive(Clone, Copy)]
enum CallbackPolicy {
    RustOwned = 0,
    JsOwned = 1,
    JsOwnedOnce = 2,
}

pub struct CallbackKey<F: ?Sized>(ObjectHandle, CallbackPolicy, PhantomData<F>);

impl<F: ?Sized> CallbackKey<F> {
    pub fn new(handle: ObjectHandle) -> Self {
        Self::js_owned(handle)
    }

    pub fn rust_owned(handle: ObjectHandle) -> Self {
        CallbackKey(handle, CallbackPolicy::RustOwned, PhantomData)
    }

    fn js_owned(handle: ObjectHandle) -> Self {
        CallbackKey(handle, CallbackPolicy::JsOwned, PhantomData)
    }

    pub fn js_owned_once(handle: ObjectHandle) -> Self {
        CallbackKey(handle, CallbackPolicy::JsOwnedOnce, PhantomData)
    }
}

impl<F: ?Sized> BinaryEncode for CallbackKey<F> {
    fn encode(self, encoder: &mut EncodedData) {
        self.0.encode(encoder);
        (self.1 as u32).encode(encoder);
    }
}

macro_rules! callback_type_def_body {
    ($encoder:expr; R = $R:ty; $($arg:ty),*) => {{
        $encoder.callback::<fn($($arg),*) -> $R>();
    }};
    ($encoder:expr; R = $R:ty; borrow_first; $($rest:ty),*) => {{
        let count: u8 = 1 $(+ {
            let _ = PhantomData::<$rest>;
            1
        })*;
        $encoder.callback_with_signature(count, |type_def| {
            type_def.borrowed_ref();
            $(<$rest as EncodeTypeDef>::encode_type_def(type_def);)*
            <$R as EncodeTypeDef>::encode_type_def(type_def);
        });
    }};
}

macro_rules! impl_callback_key_type_def {
    ($($arg:ident),*) => {
        impl<R, $($arg,)*> EncodeTypeDef for CallbackKey<fn($($arg),*) -> R>
        where
            $($arg: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; $($arg),*);
            }
        }
    };
}

macro_rules! impl_borrowed_first_callback_key_type_def {
    ($first:ident $(, $rest:ident)*) => {
        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> EncodeTypeDef for CallbackKey<fn(&$first, $($rest),*) -> R>
        where
            $first: EncodeTypeDef + 'static,
            $($rest: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; borrow_first; $($rest),*);
            }
        }
    };
}

impl_callback_key_type_def!();
impl_callback_key_type_def!(A1);
impl_callback_key_type_def!(A1, A2);
impl_callback_key_type_def!(A1, A2, A3);
impl_callback_key_type_def!(A1, A2, A3, A4);
impl_callback_key_type_def!(A1, A2, A3, A4, A5);
impl_callback_key_type_def!(A1, A2, A3, A4, A5, A6);
impl_callback_key_type_def!(A1, A2, A3, A4, A5, A6, A7);
impl_callback_key_type_def!(A1, A2, A3, A4, A5, A6, A7, A8);

impl_borrowed_first_callback_key_type_def!(A1);
impl_borrowed_first_callback_key_type_def!(A1, A2);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3, A4);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3, A4, A5);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3, A4, A5, A6);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3, A4, A5, A6, A7);
impl_borrowed_first_callback_key_type_def!(A1, A2, A3, A4, A5, A6, A7, A8);
