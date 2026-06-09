//! Arity-generic dispatch of an exported function through its arguments'
//! [`ArgAbi`] projections.
//!
//! The export proc-macro names the argument *types* and hands over the function
//! (or a small receiver-capturing closure); these traits do the decode →
//! project → call → encode threading. The continuation-passing nesting that
//! [`ArgAbi::project`]/[`ArgAbi::project_async`] require is written **once** here,
//! per arity, by [`impl_call_export!`] — mirroring how [`super::closures`] writes
//! its per-arity `Fn` upcasts — instead of being regenerated inside every export
//! wrapper.

use core::future::Future;
use core::ops::AsyncFn;
use core::pin::Pin;

use crate::__rt::alloc::{boxed::Box, vec::Vec};
use crate::JsValue;
use crate::encode::{
    Anchored, ArgAbi, BinaryEncode, BorrowScope, CallScoped, EncodeTypeDef, TypeDef,
};
use crate::ipc::{DecodeError, DecodedData, EncodedData};
use wry_bindgen_core::{JsExportSpec, JsFunctionArg, JsFunctionSignature};

use super::{ReturnAbi, ReturnAsync, ReturnSync};

/// Argument type metadata for an exported function's `Args` tuple in borrow
/// scope `S`.
#[doc(hidden)]
pub trait CallExportArgs<S: BorrowScope> {
    fn arg_types() -> Vec<TypeDef>;
}

fn export_args<Args, S>(arg_names: &'static [&'static str]) -> Vec<JsFunctionArg>
where
    Args: CallExportArgs<S>,
    S: BorrowScope,
{
    let arg_types = Args::arg_types();
    let hidden_args = arg_types.len().saturating_sub(arg_names.len());
    arg_types
        .into_iter()
        .enumerate()
        .map(|(i, ty)| JsFunctionArg {
            name: if i < hidden_args {
                ""
            } else {
                arg_names[i - hidden_args]
            },
            ty,
        })
        .collect()
}

/// Decode, project, call, and encode a synchronous export. `Args` is the tuple
/// of argument types (each implementing [`ArgAbi<S>`]); `Self` is the exported
/// `fn` or receiver-capturing closure, whose signature *is* the bound — it
/// receives one [`ArgAbi::Projected`] value per argument.
#[doc(hidden)]
pub trait CallExport<Args, S: BorrowScope>
where
    Args: CallExportArgs<S>,
{
    type ReturnMetadata: EncodeTypeDef;

    #[allow(clippy::too_many_arguments)]
    fn export_spec(
        self,
        name: &'static str,
        namespace: &'static [&'static str],
        arg_names: &'static [&'static str],
        this: bool,
        public: bool,
        start: bool,
        variadic: bool,
    ) -> JsExportSpec
    where
        Self: Sized + Copy + 'static,
    {
        let signature = JsFunctionSignature::new(
            name,
            namespace,
            export_args::<Args, S>(arg_names),
            TypeDef::of::<Self::ReturnMetadata>(),
            this,
            public,
            start,
            variadic,
        );
        let callable = self;
        JsExportSpec::new(signature, move |decoder| {
            Ok(Self::call(&callable, decoder)?)
        })
    }

    fn call(&self, decoder: &mut DecodedData) -> Result<EncodedData, DecodeError>;
}

/// The `async` counterpart of [`CallExport`]: decode the guards synchronously
/// (briefly borrowing `decoder`), then return a `'static` future — it owns the
/// guards and `self` — resolving to the `Result<JsValue, JsValue>` that backs
/// the export's `Promise`. The proc-macro turns that future into the `Promise`.
#[doc(hidden)]
pub trait CallExportAsync<Args, S: BorrowScope>
where
    Args: CallExportArgs<S>,
{
    type ReturnMetadata: EncodeTypeDef;

    #[allow(clippy::too_many_arguments)]
    fn export_spec<F, P>(
        self,
        resolve_async: F,
        name: &'static str,
        namespace: &'static [&'static str],
        arg_names: &'static [&'static str],
        this: bool,
        public: bool,
        start: bool,
        variadic: bool,
    ) -> JsExportSpec
    where
        F: Fn(Pin<Box<dyn Future<Output = Result<JsValue, JsValue>> + 'static>>) -> P + 'static,
        P: EncodeTypeDef + 'static,
        for<'a> &'a P: BinaryEncode,
        Self: Sized + Copy + 'static,
    {
        let return_type = TypeDef::of::<P>();
        let signature = JsFunctionSignature::new(
            name,
            namespace,
            export_args::<Args, S>(arg_names),
            return_type,
            this,
            public,
            start,
            variadic,
        );
        let callable = self;
        JsExportSpec::new(signature, move |decoder| {
            let result = resolve_async(Self::call_async(callable, decoder)?);
            let mut encoder = EncodedData::default();
            <&P as BinaryEncode>::encode(&result, &mut encoder);
            core::mem::forget(result);
            Ok(encoder)
        })
    }

    fn call_async(
        self,
        decoder: &mut DecodedData,
    ) -> Result<Pin<Box<dyn Future<Output = Result<JsValue, JsValue>> + 'static>>, DecodeError>;
}

/// Implement [`CallExport`] and [`CallExportAsync`] for one arity. Each argument
/// is carried as a `[lifetime ident]` pair: the lifetime gives the projected
/// argument its own independent borrow region in the `for<…>` bound (the nested
/// projection hands each argument a distinct lifetime), and the ident doubles as
/// the type parameter, the decoded-guard binding, and the projected-value
/// binding. (`macro_rules!` can't synthesize fresh lifetimes, so they are listed
/// explicitly in the per-arity invocations below, exactly as
/// [`super::closures`]'s macro lists its type-parameter pairs.)
macro_rules! impl_call_export {
    ( $( [$lt:lifetime $A:ident] )* ) => {
        impl<S, $($A,)*> CallExportArgs<S> for ($($A,)*)
        where
            S: BorrowScope,
            $( $A: ArgAbi<S>, )*
        {
            #[allow(non_snake_case)]
            fn arg_types() -> Vec<TypeDef> {
                #[allow(unused_mut)]
                let mut types = Vec::new();
                $(
                    types.push(TypeDef::of::<<$A as ArgAbi<S>>::Wire>());
                )*
                types
            }
        }

        impl<F, S, R, $($A,)*> CallExport<($($A,)*), S> for F
        where
            S: BorrowScope,
            R: ReturnSync,
            $( $A: ArgAbi<S>, )*
            F: for<$($lt),*> Fn($( <$A as ArgAbi<S>>::Projected<$lt> ),*) -> R,
        {
            type ReturnMetadata = <R as ReturnAbi<CallScoped>>::Wire;

            // Each argument's type parameter doubles as its decoded-guard and
            // projected-value binding, so the bindings are upper-case.
            #[allow(non_snake_case)]
            fn call(&self, decoder: &mut DecodedData) -> Result<EncodedData, DecodeError> {
                let _ = &decoder; // unused at arity 0
                $( let $A = <$A as ArgAbi<S>>::decode(decoder)?; )*
                let mut encoder = EncodedData::default();
                // The nested projection yields the post-call guards as a nested
                // tuple `(((), pg_last), …, pg_0)`, unwound below in declaration
                // order so each argument's `write_back` lands after the return.
                let projected_guards = impl_call_export!(@proj self encoder [$($A)*] []);
                impl_call_export!(@wb encoder projected_guards [$($A)*]);
                Ok(encoder)
            }
        }

        impl<F, S, R, $($A,)*> CallExportAsync<($($A,)*), S> for F
        where
            S: BorrowScope,
            R: ReturnAsync,
            F: Copy + 'static,
            $( $A: ArgAbi<S> + 'static, <$A as ArgAbi<S>>::Guard: 'static, )*
            F: for<$($lt),*> AsyncFn($( <$A as ArgAbi<S>>::Projected<$lt> ),*) -> R,
        {
            type ReturnMetadata = <R as ReturnAbi<Anchored>>::Wire;

            #[allow(non_snake_case)]
            fn call_async(
                self,
                decoder: &mut DecodedData,
            ) -> Result<Pin<Box<dyn Future<Output = Result<JsValue, JsValue>> + 'static>>, DecodeError> {
                let _ = &decoder; // unused at arity 0
                $( let $A = <$A as ArgAbi<S>>::decode(decoder)?; )*
                Ok(Box::pin(async move {
                    impl_call_export!(@proj_async_top self [$($A)*]).await
                }))
            }
        }
    };

    // --- sync nested projection; the expression evaluates to the nested guard
    //     tuple `(((), pg_last), …, pg_0)` (outermost project's guard outermost) ---
    (@proj $f:ident $enc:ident [] [$($c:ident)*]) => {{
        ReturnSync::return_abi($f($($c),*), &mut $enc);
    }};
    (@proj $f:ident $enc:ident [$A:ident $($rest:ident)*] [$($c:ident)*]) => {
        <$A as ArgAbi<S>>::project($A, |$A| {
            impl_call_export!(@proj $f $enc [$($rest)*] [$($c)* $A])
        })
    };

    // --- write-back in declaration order: the outer tuple field is arg 0's
    //     guard, so peel it, write it, then recurse into the inner tuple ---
    (@wb $enc:ident $pgs:ident []) => {{
        let () = $pgs;
    }};
    (@wb $enc:ident $pgs:ident [$A:ident $($rest:ident)*]) => {{
        let (__wry_inner, __wry_pg) = $pgs;
        <$A as ArgAbi<S>>::write_back(__wry_pg, &mut $enc);
        impl_call_export!(@wb $enc __wry_inner [$($rest)*]);
    }};

    // --- async nested projection; the outermost `project_async` IS the returned
    //     future (not awaited), every inner one is awaited in its parent ---
    (@proj_async_top $f:ident []) => {
        async move { ReturnAsync::into_js_result($f().await) }
    };
    (@proj_async_top $f:ident [$A:ident $($rest:ident)*]) => {
        <$A as ArgAbi<S>>::project_async($A, async move |$A| {
            impl_call_export!(@proj_async_inner $f [$($rest)*] [$A])
        })
    };
    (@proj_async_inner $f:ident [] [$($c:ident)*]) => {
        ReturnAsync::into_js_result($f($($c),*).await)
    };
    (@proj_async_inner $f:ident [$A:ident $($rest:ident)*] [$($c:ident)*]) => {
        <$A as ArgAbi<S>>::project_async($A, async move |$A| {
            impl_call_export!(@proj_async_inner $f [$($rest)*] [$($c)* $A])
        }).await
    };
}

// Exports may take any number of value arguments; cap at 16 (double the closure
// upcast macro's 8). An export with more value arguments than this would fail to
// resolve `CallExport` — extend the list to raise the cap.
impl_call_export!();
impl_call_export!(['a0 A0]);
impl_call_export!(['a0 A0]['a1 A1]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]['a11 A11]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]['a11 A11]['a12 A12]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]['a11 A11]['a12 A12]['a13 A13]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]['a11 A11]['a12 A12]['a13 A13]['a14 A14]);
impl_call_export!(['a0 A0]['a1 A1]['a2 A2]['a3 A3]['a4 A4]['a5 A5]['a6 A6]['a7 A7]['a8 A8]['a9 A9]['a10 A10]['a11 A11]['a12 A12]['a13 A13]['a14 A14]['a15 A15]);
