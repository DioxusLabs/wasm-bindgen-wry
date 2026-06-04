//! Callback registration encoding and Rust callback storage.

#![allow(clippy::type_complexity)]

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;

use super::{
    BinaryDecode, BinaryEncode, DecodeError, DecodedData, EncodeTypeDef, EncodedData, TypeDef,
    object_store::ObjectHandle,
};

type CallbackFn = dyn Fn(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError>;

#[derive(Clone)]
pub struct RustCallback {
    f: Rc<CallbackFn>,
}

impl RustCallback {
    pub fn new_fn<F>(f: F) -> Self
    where
        F: Fn(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError> + 'static,
    {
        Self { f: Rc::new(f) }
    }

    pub fn new_fn_mut<F>(f: F) -> Self
    where
        F: FnMut(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError> + 'static,
    {
        let cell = RefCell::new(f);
        Self {
            f: Rc::new(move |data: &mut DecodedData, encoder: &mut EncodedData| {
                let mut f = cell.borrow_mut();
                f(data, encoder)
            }),
        }
    }

    pub fn call(
        &self,
        data: &mut DecodedData,
        encoder: &mut EncodedData,
    ) -> Result<(), DecodeError> {
        (self.f)(data, encoder)
    }
}

const RUST_OWNED_CALLBACK_POLICY: u32 = 0;

fn encode_rust_owned_callback(handle: ObjectHandle, encoder: &mut EncodedData) {
    handle.encode(encoder);
    RUST_OWNED_CALLBACK_POLICY.encode(encoder);
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

macro_rules! insert_callback {
    ($callback:expr) => {{ crate::batch::with_runtime(|rt| rt.insert_object_box(Box::new($callback))) }};
}

macro_rules! encode_callback_ref {
    (
        impl ($($self_ty:tt)*) via *mut dyn FnMut, $ctor:ident;
        $($arg:ident),*
    ) => {
        impl<R, $($arg,)*> BinaryEncode for $($self_ty)*
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn encode(self, encoder: &mut EncodedData) {
                encoder.mark_needs_flush();

                let ptr = self as *mut dyn FnMut($($arg),*) -> R;
                let (data_ptr, vtable_ptr): (usize, usize) = unsafe { core::mem::transmute(ptr) };

                let callback = RustCallback::$ctor(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let ptr: *mut dyn FnMut($($arg),*) -> R = unsafe {
                            core::mem::transmute((data_ptr, vtable_ptr))
                        };
                        let f: &mut dyn FnMut($($arg),*) -> R = unsafe { &mut *ptr };
                        $(let $arg = <$arg as BinaryDecode>::decode(_decoder)?;)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                        Ok(())
                    },
                );
                let handle = insert_callback!(callback);
                encode_rust_owned_callback(handle, encoder);
                crate::batch::drop_rust_object(handle);
            }
        }
    };
    (
        impl ($($self_ty:tt)*) via *const dyn Fn, $ctor:ident;
        $($arg:ident),*
    ) => {
        impl<R, $($arg,)*> BinaryEncode for $($self_ty)*
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn encode(self, encoder: &mut EncodedData) {
                encoder.mark_needs_flush();

                let ptr = self as *const dyn Fn($($arg),*) -> R;
                let (data_ptr, vtable_ptr): (usize, usize) = unsafe { core::mem::transmute(ptr) };

                let callback = RustCallback::$ctor(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let ptr: *const dyn Fn($($arg),*) -> R = unsafe {
                            core::mem::transmute((data_ptr, vtable_ptr))
                        };
                        let f: &dyn Fn($($arg),*) -> R = unsafe { &*ptr };
                        $(let $arg = <$arg as BinaryDecode>::decode(_decoder)?;)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                        Ok(())
                    },
                );
                let handle = insert_callback!(callback);
                encode_rust_owned_callback(handle, encoder);
                crate::batch::drop_rust_object(handle);
            }
        }
    };
}

macro_rules! impl_callback_ref {
    ($($arg:ident),*) => {
        impl<R, $($arg,)*> EncodeTypeDef for &mut dyn FnMut($($arg),*) -> R
        where
            $($arg: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; $($arg),*);
            }
        }

        encode_callback_ref!(
            impl (&mut dyn FnMut($($arg),*) -> R) via *mut dyn FnMut, new_fn_mut;
            $($arg),*
        );

        impl<R, $($arg,)*> EncodeTypeDef for &dyn Fn($($arg),*) -> R
        where
            $($arg: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; $($arg),*);
            }
        }

        encode_callback_ref!(
            impl (&dyn Fn($($arg),*) -> R) via *const dyn Fn, new_fn;
            $($arg),*
        );

        impl<R, $($arg,)*> EncodeTypeDef for &mut dyn Fn($($arg),*) -> R
        where
            $($arg: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; $($arg),*);
            }
        }

        encode_callback_ref!(
            impl (&mut dyn Fn($($arg),*) -> R) via *const dyn Fn, new_fn;
            $($arg),*
        );
    };
}

impl_callback_ref!();
impl_callback_ref!(A1);
impl_callback_ref!(A1, A2);
impl_callback_ref!(A1, A2, A3);
impl_callback_ref!(A1, A2, A3, A4);
impl_callback_ref!(A1, A2, A3, A4, A5);
impl_callback_ref!(A1, A2, A3, A4, A5, A6);
impl_callback_ref!(A1, A2, A3, A4, A5, A6, A7);
