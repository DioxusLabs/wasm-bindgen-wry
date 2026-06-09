//! Callback key encoding and closure ABI implementations.

use alloc::boxed::Box;
use core::marker::PhantomData;

use crate::convert::{ArgAbi, CallScoped};
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use crate::value::JsValue;
use crate::{
    Closure, IntoWasmClosure, IntoWasmClosureRef, IntoWasmClosureRefMut, WasmClosureFnOnce,
    WasmClosureFnOnceAbort,
};
use wry_bindgen_core::{CallbackKey, IntoRustCallback};

use super::{BinaryDecode, BinaryEncode, EncodeTypeDef, IntoClosure, RequireFlush, TypeDef};

// Blanket impl: All Closures encode as HeapRef since they're JS heap references
impl<T: ?Sized> EncodeTypeDef for crate::ScopedClosure<'_, T> {
    fn encode_type_def(type_def: &mut TypeDef) {
        JsValue::encode_type_def(type_def);
    }
}

impl<T: ?Sized> EncodeTypeDef for &crate::ScopedClosure<'_, T> {
    fn encode_type_def(type_def: &mut TypeDef) {
        JsValue::encode_type_def(type_def);
    }
}

/// Helper macro to decode callback arguments and execute a body.
///
/// Usage: decode_args!(decoder; [type1, type2, ...] => body)
/// The body can use the type names as variables containing the decoded arguments.
macro_rules! decode_args {
    // Main entry: decode each arg and call body. A decode failure propagates as
    // `Err` from the enclosing callback closure (which returns
    // `Result<(), DecodeError>`) instead of panicking via `unwrap`.
    ($decoder:expr; [$first:ident, $($ty:ident,)*] => $body:expr) => {{
        #[allow(non_snake_case)]
        let $first = <$first as BinaryDecode>::decode($decoder)?;
        decode_args!($decoder; [$($ty,)*] => $body);
    }};
    // Nothing left to decode: run the body, then signal success to the closure.
    ($decoder:expr; [] => $body:expr) => {{
        $body;
        return Ok(());
    }};
}

macro_rules! impl_fnmut_stub {
    ($($arg:ident),*) => {
        // Implement WasmClosure trait for dyn FnMut variants
        impl<R, $($arg,)*> crate::WryWasmClosure<fn($($arg),*) -> R> for dyn FnMut($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(mut boxed: Box<Self>) -> crate::Closure<Self> {
                crate::Closure::wrap_encode_decode_mut::<fn($($arg),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = boxed($($arg),*);
                            result.encode(encoder);
                        });
                    },
                )
            }
        }

        impl<R, $($arg,)*> crate::WasmClosure for dyn FnMut($($arg),*) -> R
            where
            $($arg: 'static, )*
            R: 'static,
        {
            type Static = dyn FnMut($($arg),*) -> R;
            type AsMut = dyn FnMut($($arg),*) -> R;
        }

        // Implement WasmClosure trait for dyn Fn variants (immutable closures)
        // These CAN be called reentrantly since Fn only needs &self
        impl<R, $($arg,)*> crate::WryWasmClosure<fn($($arg),*) -> R> for dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(boxed: Box<Self>) -> crate::Closure<Self> {
                crate::Closure::wrap_encode_decode::<fn($($arg),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = boxed($($arg),*);
                            result.encode(encoder);
                        });
                    }
                )
            }
        }

        impl<R, $($arg,)*> crate::WasmClosure for dyn Fn($($arg),*) -> R
            where
            $($arg: 'static, )*
            R: 'static,
        {
            type Static = dyn Fn($($arg),*) -> R;
            type AsMut = dyn FnMut($($arg),*) -> R;
        }

        // IntoClosure for F: FnMut -> Closure<dyn FnMut>
        impl<R, F, $($arg,)*> IntoClosure<fn($($arg),*) -> R, crate::Closure<dyn FnMut($($arg),*) -> R>> for F
            where F: FnMut($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(mut self) -> crate::Closure<dyn FnMut($($arg),*) -> R> {
                crate::Closure::wrap_encode_decode_mut::<fn($($arg),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = self($($arg),*);
                            result.encode(encoder);
                        });
                    },
                )
            }
        }

        // IntoClosure for F: Fn -> Closure<dyn Fn>
        impl<R, F, $($arg,)*> IntoClosure<fn($($arg),*) -> R, crate::Closure<dyn Fn($($arg),*) -> R>> for F
            where F: Fn($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(self) -> crate::Closure<dyn Fn($($arg),*) -> R> {
                crate::Closure::wrap_encode_decode::<fn($($arg),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        // Decode arguments and call the closure
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = self($($arg),*);
                            result.encode(encoder);
                        });
                    },
                )
            }
        }

        impl<R, F, $($arg,)*> IntoWasmClosure<dyn FnMut($($arg),*) -> R> for F
            where F: FnMut($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure(self) -> crate::Closure<dyn FnMut($($arg),*) -> R> {
                <F as IntoClosure<fn($($arg),*) -> R, crate::Closure<dyn FnMut($($arg),*) -> R>>>::into_closure(self)
            }

            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn FnMut($($arg),*) -> R> {
                <F as IntoWasmClosure<dyn FnMut($($arg),*) -> R>>::into_closure(*self)
            }
        }

        impl<R, $($arg,)*> IntoWasmClosure<dyn FnMut($($arg),*) -> R> for dyn FnMut($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn FnMut($($arg),*) -> R> {
                <Self as crate::WryWasmClosure<fn($($arg),*) -> R>>::into_js_closure(self)
            }
        }

        impl<R, F, $($arg,)*> IntoWasmClosure<dyn Fn($($arg),*) -> R> for F
            where F: Fn($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure(self) -> crate::Closure<dyn Fn($($arg),*) -> R> {
                <F as IntoClosure<fn($($arg),*) -> R, crate::Closure<dyn Fn($($arg),*) -> R>>>::into_closure(self)
            }

            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn Fn($($arg),*) -> R> {
                <F as IntoWasmClosure<dyn Fn($($arg),*) -> R>>::into_closure(*self)
            }
        }

        impl<R, $($arg,)*> IntoWasmClosure<dyn Fn($($arg),*) -> R> for dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn Fn($($arg),*) -> R> {
                <Self as crate::WryWasmClosure<fn($($arg),*) -> R>>::into_js_closure(self)
            }
        }

        impl<R, F, $($arg,)*> IntoWasmClosureRef<dyn Fn($($arg),*) -> R> for F
            where F: Fn($($arg),*) -> R,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_scoped_closure_ref<'a>(t: &'a Self) -> crate::ScopedClosure<'a, <dyn Fn($($arg),*) -> R as crate::WasmClosure>::Static> {
                let t: &(dyn Fn($($arg),*) -> R) = t;
                let callback = IntoRustCallback::into_rust_callback(t);
                let handle = crate::object_store::insert_object(callback);
                let value = crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::JsValue>(
                    CallbackKey::rust_owned(handle),
                );
                crate::ScopedClosure {
                    _phantom: PhantomData,
                    callback: crate::closure::CallbackOwnership::owned(handle),
                    value,
                }
            }
        }

        impl<R, $($arg,)*> IntoWasmClosureRef<dyn Fn($($arg),*) -> R> for dyn Fn($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_scoped_closure_ref<'a>(t: &'a Self) -> crate::ScopedClosure<'a, <dyn Fn($($arg),*) -> R as crate::WasmClosure>::Static> {
                let callback = IntoRustCallback::into_rust_callback(t);
                let handle = crate::object_store::insert_object(callback);
                let value = crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::JsValue>(
                    CallbackKey::rust_owned(handle),
                );
                crate::ScopedClosure {
                    _phantom: PhantomData,
                    callback: crate::closure::CallbackOwnership::owned(handle),
                    value,
                }
            }
        }

        impl<R, F, $($arg,)*> IntoWasmClosureRefMut<dyn FnMut($($arg),*) -> R> for F
            where F: FnMut($($arg),*) -> R,
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_scoped_closure_ref_mut<'a>(t: &'a mut Self) -> crate::ScopedClosure<'a, <dyn FnMut($($arg),*) -> R as crate::WasmClosure>::Static> {
                let t: &mut dyn FnMut($($arg),*) -> R = t;
                let callback = IntoRustCallback::into_rust_callback(t);
                let handle = crate::object_store::insert_object(callback);
                let value = crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::JsValue>(
                    CallbackKey::rust_owned(handle),
                );
                crate::ScopedClosure {
                    _phantom: PhantomData,
                    callback: crate::closure::CallbackOwnership::owned(handle),
                    value,
                }
            }
        }

        impl<R, $($arg,)*> IntoWasmClosureRefMut<dyn FnMut($($arg),*) -> R> for dyn FnMut($($arg),*) -> R
            where
            $($arg: BinaryDecode + EncodeTypeDef + 'static, )*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_scoped_closure_ref_mut<'a>(t: &'a mut Self) -> crate::ScopedClosure<'a, <dyn FnMut($($arg),*) -> R as crate::WasmClosure>::Static> {
                let callback = IntoRustCallback::into_rust_callback(t);
                let handle = crate::object_store::insert_object(callback);
                let value = crate::__rt::wbg_cast::<CallbackKey<fn($($arg),*) -> R>, crate::JsValue>(
                    CallbackKey::rust_owned(handle),
                );
                crate::ScopedClosure {
                    _phantom: PhantomData,
                    callback: crate::closure::CallbackOwnership::owned(handle),
                    value,
                }
            }
        }
    };
}

impl_fnmut_stub!();
impl_fnmut_stub!(A1);
impl_fnmut_stub!(A1, A2);
impl_fnmut_stub!(A1, A2, A3);
impl_fnmut_stub!(A1, A2, A3, A4);
impl_fnmut_stub!(A1, A2, A3, A4, A5);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6, A7);
impl_fnmut_stub!(A1, A2, A3, A4, A5, A6, A7, A8);

// `AssertUnwindSafe<F>` is a transparent unwind-safety assertion, so its closure
// conversions forward to the inner `F`. These explicit impls cover the case
// where it wraps a closure with arguments (std only implements the `Fn` traits
// for `AssertUnwindSafe` with zero arguments), which is how upstream tests pass
// a non-`UnwindSafe` capture through `Closure::borrow`/`wrap`.
impl<T: ?Sized, F> IntoWasmClosure<T> for core::panic::AssertUnwindSafe<F>
where
    F: IntoWasmClosure<T>,
{
    fn into_closure(self) -> crate::Closure<T> {
        F::into_closure(self.0)
    }

    fn into_closure_box(self: alloc::boxed::Box<Self>) -> crate::Closure<T> {
        F::into_closure(self.0)
    }
}

impl<T: ?Sized, F> IntoWasmClosureRef<T> for core::panic::AssertUnwindSafe<F>
where
    F: IntoWasmClosureRef<T>,
{
    fn into_scoped_closure_ref<'a>(t: &'a Self) -> crate::ScopedClosure<'a, T::Static>
    where
        T: crate::WasmClosure,
    {
        F::into_scoped_closure_ref(&t.0)
    }
}

impl<T: ?Sized, F> IntoWasmClosureRefMut<T> for core::panic::AssertUnwindSafe<F>
where
    F: IntoWasmClosureRefMut<T>,
{
    fn into_scoped_closure_ref_mut<'a>(t: &'a mut Self) -> crate::ScopedClosure<'a, T::Static>
    where
        T: crate::WasmClosure,
    {
        F::into_scoped_closure_ref_mut(&mut t.0)
    }
}

/// Marker type for closures that borrow the first argument.
pub struct BorrowedFirstArg;

/// Macro to implement WasmClosure and IntoClosure for closures that borrow the first argument.
/// This uses `ArgAbi<CallScoped>` for the first arg and `BinaryDecode` for the rest.
macro_rules! impl_fnmut_stub_ref {
    ($first:ident $(, $rest:ident)*) => {
        // WasmClosure for dyn FnMut(&First, ...) -> R
        impl<R, $first, $($rest,)*> crate::WryWasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for dyn FnMut(&$first, $($rest),*) -> R
            where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(mut boxed: Box<Self>) -> crate::Closure<Self> {
                crate::Closure::wrap_encode_decode_mut::<fn(&$first, $($rest),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder)?;)*
                        let result = boxed(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        // Trait objects like `dyn FnMut(&Event)` are commonly inferred as
        // higher-ranked over the borrowed argument lifetime.
        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> crate::WasmClosure for dyn FnMut(&$first, $($rest),*) -> R
            where
            $first: 'static,
            $($rest: 'static,)*
            R: 'static,
        {
            type Static = dyn FnMut(&$first, $($rest),*) -> R;
            type AsMut = dyn FnMut(&$first, $($rest),*) -> R;
        }

        // WasmClosure for dyn Fn(&First, ...) -> R (supports reentrant calls)
        impl<R, $first, $($rest,)*> crate::WryWasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)> for dyn Fn(&$first, $($rest),*) -> R
            where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_js_closure(boxed: Box<Self>) -> crate::Closure<Self> {
                crate::Closure::wrap_encode_decode::<fn(&$first, $($rest),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder)?;)*
                        let result = boxed(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> crate::WasmClosure for dyn Fn(&$first, $($rest),*) -> R
            where
            $first: 'static,
            $($rest: 'static,)*
            R: 'static,
        {
            type Static = dyn Fn(&$first, $($rest),*) -> R;
            type AsMut = dyn FnMut(&$first, $($rest),*) -> R;
        }

        // IntoClosure for F: FnMut(&First, ...) -> R -> Closure<dyn FnMut(&First, ...) -> R>
        impl<R, F, $first, $($rest,)*> IntoClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R), crate::Closure<dyn FnMut(&$first, $($rest),*) -> R>> for F
            where F: FnMut(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(mut self) -> crate::Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                crate::Closure::wrap_encode_decode_mut::<fn(&$first, $($rest),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder)?;)*
                        let result = self(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        // IntoClosure for F: Fn(&First, ...) -> R -> Closure<dyn Fn(&First, ...) -> R>
        impl<R, F, $first, $($rest,)*> IntoClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R), crate::Closure<dyn Fn(&$first, $($rest),*) -> R>> for F
            where F: Fn(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused)]
            fn into_closure(self) -> crate::Closure<dyn Fn(&$first, $($rest),*) -> R> {
                crate::Closure::wrap_encode_decode::<fn(&$first, $($rest),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder)?;)*
                        let result = self(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        #[allow(coherence_leak_check)]
        impl<R, F, $first, $($rest,)*> IntoWasmClosure<dyn FnMut(&$first, $($rest),*) -> R> for F
            where F: FnMut(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure(self) -> crate::Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                <F as IntoClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R), crate::Closure<dyn FnMut(&$first, $($rest),*) -> R>>>::into_closure(self)
            }

            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                <F as IntoWasmClosure<dyn FnMut(&$first, $($rest),*) -> R>>::into_closure(*self)
            }
        }

        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> IntoWasmClosure<dyn FnMut(&$first, $($rest),*) -> R> for dyn FnMut(&$first, $($rest),*) -> R
            where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                <Self as crate::WryWasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)>>::into_js_closure(self)
            }
        }

        #[allow(coherence_leak_check)]
        impl<R, F, $first, $($rest,)*> IntoWasmClosure<dyn Fn(&$first, $($rest),*) -> R> for F
            where F: Fn(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure(self) -> crate::Closure<dyn Fn(&$first, $($rest),*) -> R> {
                <F as IntoClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R), crate::Closure<dyn Fn(&$first, $($rest),*) -> R>>>::into_closure(self)
            }

            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn Fn(&$first, $($rest),*) -> R> {
                <F as IntoWasmClosure<dyn Fn(&$first, $($rest),*) -> R>>::into_closure(*self)
            }
        }

        #[allow(coherence_leak_check)]
        impl<R, $first, $($rest,)*> IntoWasmClosure<dyn Fn(&$first, $($rest),*) -> R> for dyn Fn(&$first, $($rest),*) -> R
            where
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            fn into_closure_box(self: Box<Self>) -> crate::Closure<dyn Fn(&$first, $($rest),*) -> R> {
                <Self as crate::WryWasmClosure<(BorrowedFirstArg, fn(&$first, $($rest),*) -> R)>>::into_js_closure(self)
            }
        }
    };
}

impl_fnmut_stub_ref!(A1);
impl_fnmut_stub_ref!(A1, A2);
impl_fnmut_stub_ref!(A1, A2, A3);
impl_fnmut_stub_ref!(A1, A2, A3, A4);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5, A6);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5, A6, A7);
impl_fnmut_stub_ref!(A1, A2, A3, A4, A5, A6, A7, A8);

/// Macro to implement WasmClosureFnOnce for FnOnce closures of various arities.
/// This wraps an FnOnce in an FnMut that panics if called more than once.
macro_rules! impl_fn_once {
    ($($arg:ident),*) => {
        impl<R, F, $($arg,)*> WasmClosureFnOnce<dyn FnMut($($arg),*) -> R, fn($($arg),*) -> R, R> for F
        where
            F: FnOnce($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut($($arg),*) -> R> {
                // Use Option to allow taking the FnOnce
                let mut me = Some(self);
                // Register the callback using the same pattern as impl_fnmut_stub
                crate::Closure::wrap_once_encode_decode_mut::<fn($($arg),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let f = me.take().expect("FnOnce closure called more than once");
                        decode_args!(decoder; [$($arg,)*] => {
                            let result = f($($arg),*);
                            result.encode(encoder);
                        });
                    },
                )
            }
        }

        impl<R, F, $($arg,)*> WasmClosureFnOnceAbort<dyn FnMut($($arg),*) -> R, fn($($arg),*) -> R, R> for F
        where
            F: FnOnce($($arg),*) -> R + 'static,
            $($arg: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut($($arg),*) -> R> {
                <F as WasmClosureFnOnce<dyn FnMut($($arg),*) -> R, fn($($arg),*) -> R, R>>::into_closure(self)
            }
        }
    };
}

impl_fn_once!();
impl_fn_once!(A1);
impl_fn_once!(A1, A2);
impl_fn_once!(A1, A2, A3);
impl_fn_once!(A1, A2, A3, A4);
impl_fn_once!(A1, A2, A3, A4, A5);
impl_fn_once!(A1, A2, A3, A4, A5, A6);
impl_fn_once!(A1, A2, A3, A4, A5, A6, A7);
impl_fn_once!(A1, A2, A3, A4, A5, A6, A7, A8);

/// Macro to implement WasmClosureFnOnce for FnOnce closures that borrow the first argument.
/// This uses `ArgAbi<CallScoped>` for the first arg and `BinaryDecode` for the rest.
macro_rules! impl_fn_once_ref {
    ($first:ident $(, $rest:ident)*) => {
        impl<R, F, $first, $($rest,)*> WasmClosureFnOnce<dyn FnMut(&$first, $($rest),*) -> R, (BorrowedFirstArg, fn(&$first, $($rest),*) -> R), R> for F
        where
            F: FnOnce(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                let mut me = Some(self);
                crate::Closure::wrap_once_encode_decode_mut::<fn(&$first, $($rest),*) -> R>(
                    move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
                        let f = me.take().expect("FnOnce closure called more than once");
                        let __guard = <&'static $first as ArgAbi<CallScoped>>::decode(decoder)?;
                        $(let $rest = <$rest as BinaryDecode>::decode(decoder)?;)*
                        let result = f(&*__guard, $($rest),*);
                        result.encode(encoder);
                        Ok(())
                    },
                )
            }
        }

        impl<R, F, $first, $($rest,)*> WasmClosureFnOnceAbort<dyn FnMut(&$first, $($rest),*) -> R, (BorrowedFirstArg, fn(&$first, $($rest),*) -> R), R> for F
        where
            F: FnOnce(&$first, $($rest),*) -> R + 'static,
            &'static $first: ArgAbi<CallScoped>,
            <&'static $first as ArgAbi<CallScoped>>::Guard: core::ops::Deref<Target = $first>,
            $first: 'static,
            $($rest: BinaryDecode + EncodeTypeDef + 'static,)*
            R: BinaryEncode + EncodeTypeDef + 'static,
        {
            #[allow(non_snake_case)]
            #[allow(unused_variables)]
            fn into_closure(self) -> Closure<dyn FnMut(&$first, $($rest),*) -> R> {
                <F as WasmClosureFnOnce<dyn FnMut(&$first, $($rest),*) -> R, (BorrowedFirstArg, fn(&$first, $($rest),*) -> R), R>>::into_closure(self)
            }
        }
    };
}

impl_fn_once_ref!(A1);
impl_fn_once_ref!(A1, A2);
impl_fn_once_ref!(A1, A2, A3);
impl_fn_once_ref!(A1, A2, A3, A4);
impl_fn_once_ref!(A1, A2, A3, A4, A5);
impl_fn_once_ref!(A1, A2, A3, A4, A5, A6);
impl_fn_once_ref!(A1, A2, A3, A4, A5, A6, A7);
impl_fn_once_ref!(A1, A2, A3, A4, A5, A6, A7, A8);

impl<F: ?Sized> BinaryDecode for crate::Closure<F> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        // Decode the JsValue wrapping the closure
        let value = <crate::JsValue as BinaryDecode>::decode(decoder)?;
        Ok(Self {
            _phantom: PhantomData,
            callback: crate::closure::CallbackOwnership::None,
            value,
        })
    }
}

impl<F: ?Sized> BinaryEncode for crate::Closure<F> {
    fn encode(mut self, encoder: &mut EncodedData) {
        if self.callback.needs_flush() {
            RequireFlush.encode(encoder);
        }
        // Hand the closure off to JS: ScopedClosure::drop must not release the
        // Rust callback object. JsValue::drop still queues the heap-ref release.
        self.callback.detach();
        (&self.value).encode(encoder);
    }
}

impl<F: ?Sized> BinaryEncode for &crate::ScopedClosure<'_, F> {
    fn encode(self, encoder: &mut EncodedData) {
        if self.callback.needs_flush() {
            RequireFlush.encode(encoder);
        }
        // Encode the JsValue
        (&self.value).encode(encoder);
    }
}
