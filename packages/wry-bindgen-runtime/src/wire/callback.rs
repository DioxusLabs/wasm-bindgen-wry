//! Callback registration encoding and Rust callback storage.

#![allow(clippy::type_complexity)]

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::marker::PhantomData;

use super::{
    ArgAbi, BinaryDecode, BinaryEncode, CallScoped, DecodeError, DecodedData, EncodeTypeDef,
    EncodedData, TypeDef, object_store::ObjectHandle,
};

type CallbackFn = dyn Fn(&mut DecodedData, &mut EncodedData) -> Result<(), DecodeError>;

#[derive(Clone, Copy)]
enum CallbackPolicy {
    RustOwned = 0,
    JsOwned = 1,
    JsOwnedOnce = 2,
}

#[doc(hidden)]
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
                // A `FnMut` callback that is invoked again while already running
                // (re-entrancy) surfaces as a catchable error, matching
                // wasm-bindgen, rather than panicking on the borrow.
                let mut f = cell.try_borrow_mut().map_err(|_| {
                    DecodeError::custom("closure invoked recursively or after being dropped")
                })?;
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

#[doc(hidden)]
pub trait IntoRustCallback {
    fn into_rust_callback(self) -> RustCallback;
}

fn encode_rust_owned_callback(handle: ObjectHandle, encoder: &mut EncodedData) {
    handle.encode(encoder);
    (CallbackPolicy::RustOwned as u32).encode(encoder);
}

macro_rules! callback_type_def_body {
    ($encoder:expr; R = $R:ty; $($arg:ty),*) => {{
        $encoder.callback::<fn($($arg),*) -> $R>();
    }};
    ($encoder:expr; R = $R:ty; borrow_first = $first:ty; $($rest:ty),*) => {{
        let count: u8 = 1 $(+ {
            let _ = PhantomData::<$rest>;
            1
        })*;
        $encoder.callback_with_signature(count, |type_def| {
            <<&'static $first as ArgAbi<CallScoped>>::Wire as EncodeTypeDef>::encode_type_def(type_def);
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
            &'static $first: ArgAbi<CallScoped>,
            $first: 'static,
            $($rest: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; borrow_first = $first; $($rest),*);
            }
        }
    };
}

macro_rules! insert_callback {
    ($callback:expr) => {{ crate::batch::with_runtime(|rt| rt.insert_object_box(Box::new($callback))) }};
}

unsafe fn scoped_callback_ref<From, To>(value: &From) -> &'static To
where
    From: ?Sized,
    To: ?Sized,
{
    assert_eq!(core::mem::size_of::<&From>(), core::mem::size_of::<&To>());
    // SAFETY: callers pass the same callback trait object with its object
    // lifetime changed to `'static`.
    unsafe { core::mem::transmute_copy::<&From, &'static To>(&value) }
}

unsafe fn scoped_callback_ref_mut<From, To>(value: &mut From) -> &'static mut To
where
    From: ?Sized,
    To: ?Sized,
{
    assert_eq!(
        core::mem::size_of::<&mut From>(),
        core::mem::size_of::<&mut To>()
    );
    // SAFETY: callers pass the same callback trait object with its object
    // lifetime changed to `'static`.
    unsafe { core::mem::transmute_copy::<&mut From, &'static mut To>(&value) }
}

macro_rules! impl_into_rust_callback_ref {
    ($($arg:ident),*) => {
        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $($arg,)*> IntoRustCallback
            for &'borrow mut (dyn FnMut($($arg),*) -> R + 'object)
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated, and it is the only callback path holding this
                // mutable borrow while registered.
                let f: &'static mut (dyn FnMut($($arg),*) -> R + 'static) =
                    unsafe { scoped_callback_ref_mut(self) };
                RustCallback::new_fn_mut(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        $(let $arg = <$arg as BinaryDecode>::decode(_decoder)?;)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $($arg,)*> IntoRustCallback
            for &'borrow (dyn Fn($($arg),*) -> R + 'object)
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated.
                let f: &'static (dyn Fn($($arg),*) -> R + 'static) =
                    unsafe { scoped_callback_ref(self) };
                RustCallback::new_fn(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        $(let $arg = <$arg as BinaryDecode>::decode(_decoder)?;)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $($arg,)*> IntoRustCallback
            for &'borrow mut (dyn Fn($($arg),*) -> R + 'object)
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated.
                let f: &'static (dyn Fn($($arg),*) -> R + 'static) =
                    unsafe { scoped_callback_ref(&*self) };
                RustCallback::new_fn(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        $(let $arg = <$arg as BinaryDecode>::decode(_decoder)?;)*
                        let result = f($($arg),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }
    };
}

macro_rules! impl_into_rust_callback_borrow_first {
    ($first:ident $(, $rest:ident)*) => {
        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $first, $($rest,)*> IntoRustCallback
            for &'borrow mut (dyn FnMut(&$first, $($rest),*) -> R + 'object)
        where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated, and it is the only callback path holding this
                // mutable borrow while registered.
                let f: &'static mut (dyn FnMut(&$first, $($rest),*) -> R + 'static) =
                    unsafe { scoped_callback_ref_mut(self) };
                RustCallback::new_fn_mut(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(_decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(_decoder)?;)*
                        let result = f(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $first, $($rest,)*> IntoRustCallback
            for &'borrow (dyn Fn(&$first, $($rest),*) -> R + 'object)
        where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated.
                let f: &'static (dyn Fn(&$first, $($rest),*) -> R + 'static) =
                    unsafe { scoped_callback_ref(self) };
                RustCallback::new_fn(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(_decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(_decoder)?;)*
                        let result = f(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<'borrow, 'object, R, $first, $($rest,)*> IntoRustCallback
            for &'borrow mut (dyn Fn(&$first, $($rest),*) -> R + 'object)
        where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn into_rust_callback(self) -> RustCallback {
                // SAFETY: this changes only the trait-object lifetime. The
                // resulting callback is owned by an operation- or
                // closure-scoped handle that is dropped before `self` can be
                // invalidated.
                let f: &'static (dyn Fn(&$first, $($rest),*) -> R + 'static) =
                    unsafe { scoped_callback_ref(&*self) };
                RustCallback::new_fn(
                    move |_decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(_decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(_decoder)?;)*
                        let result = f(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }
    };
}

macro_rules! encode_callback_ref {
    (
        impl ($($self_ty:tt)*);
        $($arg:ident),*
    ) => {
        #[allow(coherence_leak_check)]
        impl<R, $($arg,)*> BinaryEncode for $($self_ty)*
        where
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn encode(self, encoder: &mut EncodedData) {
                encoder.mark_needs_flush();

                let callback = IntoRustCallback::into_rust_callback(self);
                let handle = insert_callback!(callback);
                encode_rust_owned_callback(handle, encoder);
                crate::batch::drop_rust_object(handle);
            }
        }
    };
}

macro_rules! impl_callback_ref {
    ($($arg:ident),*) => {
        #[allow(coherence_leak_check)]
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
            impl (&mut dyn FnMut($($arg),*) -> R);
            $($arg),*
        );

        #[allow(coherence_leak_check)]
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
            impl (&dyn Fn($($arg),*) -> R);
            $($arg),*
        );

        #[allow(coherence_leak_check)]
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
            impl (&mut dyn Fn($($arg),*) -> R);
            $($arg),*
        );
    };
}

// Encode a borrowed `&dyn Fn`/`&mut dyn FnMut` whose FIRST argument is a
// reference (`&First`). The first arg is decoded through `ArgAbi<CallScoped>`
// (it rides JS's borrow stack or object-store checkout and is anchored for
// the call), the rest by value.
macro_rules! encode_callback_borrow_first {
    (
        impl ($($self_ty:tt)*);
        $first:ident $(, $rest:ident)*
    ) => {
        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> BinaryEncode for $($self_ty)*
        where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            fn encode(self, encoder: &mut EncodedData) {
                encoder.mark_needs_flush();

                let callback = IntoRustCallback::into_rust_callback(self);
                let handle = insert_callback!(callback);
                encode_rust_owned_callback(handle, encoder);
                crate::batch::drop_rust_object(handle);
            }
        }
    };
}

macro_rules! impl_callback_borrow_first {
    ($first:ident $(, $rest:ident)*) => {
        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> EncodeTypeDef for &mut dyn FnMut(&$first, $($rest),*) -> R
        where
            &'static $first: ArgAbi<CallScoped>,
            $first: 'static,
            $($rest: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; borrow_first = $first; $($rest),*);
            }
        }

        encode_callback_borrow_first!(
            impl (&mut dyn FnMut(&$first, $($rest),*) -> R);
            $first $(, $rest)*
        );

        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> EncodeTypeDef for &dyn Fn(&$first, $($rest),*) -> R
        where
            &'static $first: ArgAbi<CallScoped>,
            $first: 'static,
            $($rest: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; borrow_first = $first; $($rest),*);
            }
        }

        encode_callback_borrow_first!(
            impl (&dyn Fn(&$first, $($rest),*) -> R);
            $first $(, $rest)*
        );

        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> EncodeTypeDef for &mut dyn Fn(&$first, $($rest),*) -> R
        where
            &'static $first: ArgAbi<CallScoped>,
            $first: 'static,
            $($rest: EncodeTypeDef + 'static,)*
            R: EncodeTypeDef + 'static,
        {
            fn encode_type_def(encoder: &mut TypeDef) {
                callback_type_def_body!(encoder; R = R; borrow_first = $first; $($rest),*);
            }
        }

        encode_callback_borrow_first!(
            impl (&mut dyn Fn(&$first, $($rest),*) -> R);
            $first $(, $rest)*
        );
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

impl_callback_borrow_first!(A1);
impl_callback_borrow_first!(A1, A2);
impl_callback_borrow_first!(A1, A2, A3);
impl_callback_borrow_first!(A1, A2, A3, A4);
impl_callback_borrow_first!(A1, A2, A3, A4, A5);
impl_callback_borrow_first!(A1, A2, A3, A4, A5, A6);
impl_callback_borrow_first!(A1, A2, A3, A4, A5, A6, A7);
impl_callback_borrow_first!(A1, A2, A3, A4, A5, A6, A7, A8);

impl_into_rust_callback_borrow_first!(A1);
impl_into_rust_callback_borrow_first!(A1, A2);
impl_into_rust_callback_borrow_first!(A1, A2, A3);
impl_into_rust_callback_borrow_first!(A1, A2, A3, A4);
impl_into_rust_callback_borrow_first!(A1, A2, A3, A4, A5);
impl_into_rust_callback_borrow_first!(A1, A2, A3, A4, A5, A6);
impl_into_rust_callback_borrow_first!(A1, A2, A3, A4, A5, A6, A7);
impl_into_rust_callback_borrow_first!(A1, A2, A3, A4, A5, A6, A7, A8);

impl_into_rust_callback_ref!();
impl_into_rust_callback_ref!(A1);
impl_into_rust_callback_ref!(A1, A2);
impl_into_rust_callback_ref!(A1, A2, A3);
impl_into_rust_callback_ref!(A1, A2, A3, A4);
impl_into_rust_callback_ref!(A1, A2, A3, A4, A5);
impl_into_rust_callback_ref!(A1, A2, A3, A4, A5, A6);
impl_into_rust_callback_ref!(A1, A2, A3, A4, A5, A6, A7);
impl_into_rust_callback_ref!(A1, A2, A3, A4, A5, A6, A7, A8);

impl_callback_ref!();
impl_callback_ref!(A1);
impl_callback_ref!(A1, A2);
impl_callback_ref!(A1, A2, A3);
impl_callback_ref!(A1, A2, A3, A4);
impl_callback_ref!(A1, A2, A3, A4, A5);
impl_callback_ref!(A1, A2, A3, A4, A5, A6);
impl_callback_ref!(A1, A2, A3, A4, A5, A6, A7);
impl_callback_ref!(A1, A2, A3, A4, A5, A6, A7, A8);
