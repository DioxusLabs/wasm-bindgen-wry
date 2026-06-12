//! Arity-generic dispatch of an exported function through its arguments'
//! [`ArgAbi`] projections.
//!
//! The export proc-macro names the argument *types* and hands over the function
//! (or a small receiver-capturing closure); these traits do the decode →
//! project → call → encode threading, written **once** here, per arity, by
//! [`impl_call_export!`] — mirroring how [`super::closures`] writes its
//! per-arity `Fn` upcasts — instead of being regenerated inside every export
//! wrapper. Because [`ArgAbi::decode`] splits each argument into a by-value
//! part and a caller-owned anchor, the same [`ArgAbi::project`] serves
//! both shapes: the sync wrapper keeps the anchors on its stack, while
//! the async wrapper moves them into the returned future so the projected
//! borrows live across `.await`s.

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

/// The `async` counterpart of [`CallExport`]: decode the arguments synchronously
/// (briefly borrowing `decoder`), then return a `'static` future — it owns the
/// anchors and `self` — resolving to the `Result<JsValue, JsValue>` that backs
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
/// is carried as a `[lifetime ident ident]` triple: the lifetime gives the
/// projected argument its own independent borrow region in the `for<…>` bound,
/// the first ident doubles as the type parameter and the by-value binding,
/// and the second ident binds the anchor the projected borrow lives in.
/// (`macro_rules!` can't synthesize fresh lifetimes or idents, so they are
/// listed explicitly in the per-arity invocations below, exactly as
/// [`super::closures`]'s macro lists its type-parameter pairs.)
macro_rules! impl_call_export {
    ( $( [$lt:lifetime $A:ident $G:ident] )* ) => {
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

            // Each argument's type parameter doubles as its by-value
            // binding, so the bindings are upper-case.
            #[allow(non_snake_case)]
            fn call(&self, decoder: &mut DecodedData) -> Result<EncodedData, DecodeError> {
                let _ = &decoder; // unused at arity 0
                $( let ($A, mut $G) = <$A as ArgAbi<S>>::decode(decoder)?; )*
                let mut encoder = EncodedData::default();
                ReturnSync::return_abi(
                    self($( <$A as ArgAbi<S>>::project($A, &mut $G) ),*),
                    &mut encoder,
                );
                // Write-back in declaration order, after the return payload.
                $( <$A as ArgAbi<S>>::write_back($G, &mut encoder); )*
                Ok(encoder)
            }
        }

        impl<F, S, R, $($A,)*> CallExportAsync<($($A,)*), S> for F
        where
            S: BorrowScope,
            R: ReturnAsync,
            F: Copy + 'static,
            $(
                $A: ArgAbi<S> + 'static,
                <$A as ArgAbi<S>>::Value: 'static,
                <$A as ArgAbi<S>>::Anchor: 'static,
            )*
            F: for<$($lt),*> AsyncFn($( <$A as ArgAbi<S>>::Projected<$lt> ),*) -> R,
        {
            type ReturnMetadata = <R as ReturnAbi<Anchored>>::Wire;

            #[allow(non_snake_case)]
            fn call_async(
                self,
                decoder: &mut DecodedData,
            ) -> Result<Pin<Box<dyn Future<Output = Result<JsValue, JsValue>> + 'static>>, DecodeError> {
                let _ = &decoder; // unused at arity 0
                $( let ($A, mut $G) = <$A as ArgAbi<S>>::decode(decoder)?; )*
                // The anchors move into the future, so each projected
                // borrow lives across the `.await` until the call completes.
                Ok(Box::pin(async move {
                    ReturnAsync::into_js_result(
                        self($( <$A as ArgAbi<S>>::project($A, &mut $G) ),*).await,
                    )
                }))
            }
        }
    };
}

// Exports may take any number of value arguments; cap at 16 (double the closure
// upcast macro's 8). An export with more value arguments than this would fail to
// resolve `CallExport` — extend the list to raise the cap.
impl_call_export!();
impl_call_export!(['a0 A0 G0]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]['a11 A11 G11]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]['a11 A11 G11]['a12 A12 G12]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]['a11 A11 G11]['a12 A12 G12]['a13 A13 G13]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]['a11 A11 G11]['a12 A12 G12]['a13 A13 G13]['a14 A14 G14]);
impl_call_export!(['a0 A0 G0]['a1 A1 G1]['a2 A2 G2]['a3 A3 G3]['a4 A4 G4]['a5 A5 G5]['a6 A6 G6]['a7 A7 G7]['a8 A8 G8]['a9 A9 G9]['a10 A10 G10]['a11 A11 G11]['a12 A12 G12]['a13 A13 G13]['a14 A14 G14]['a15 A15 G15]);
