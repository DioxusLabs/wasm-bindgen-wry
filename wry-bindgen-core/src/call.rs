//! Typed JavaScript function handles.
//!
//! [`JsFunction`] resolves a [`JsFunctionSpec`] on first use and caches the
//! handle. Its `.call(args)` performs the round-trip; the resolved function id
//! and the cached type-id protocol are private to this module.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use core::marker::PhantomData;

use once_cell::sync::OnceCell;
use wry_bindgen_abi::{BinaryEncode, EncodeTypeDef, FunctionTypeInfo, TypeDef};
use wry_bindgen_abi::{EncodedData, JsFunctionSpec};

use crate::BatchableResult;
use crate::runtime::{run_js_sync, with_backend};

/// Encode the cached type-definition prelude for a function call.
///
/// On first encounter, emits a full inline type definition and records its id
/// on the encoder's pending-ack list. Once JS has acked the definition,
/// subsequent calls emit the cached id only.
fn encode_function_types<Signature: EncodeTypeDef>(encoder: &mut EncodedData) {
    let type_def = TypeDef::of::<Signature>();
    with_backend(|state| {
        let (id, can_use_cached) = state.get_or_create_type_id(&type_def);
        FunctionTypeInfo::new(id, can_use_cached, &type_def).encode(encoder);
    });
}

/// A typed JS function that resolves from the active runtime on first use.
pub struct JsFunction<F> {
    spec: JsFunctionSpec,
    inner: OnceCell<u32>,
    function: PhantomData<F>,
}

impl<F> JsFunction<F> {
    pub const fn new(spec: JsFunctionSpec) -> Self {
        Self {
            spec,
            inner: OnceCell::new(),
            function: PhantomData,
        }
    }

    fn id(&self) -> u32 {
        *self
            .inner
            .get_or_init(|| with_backend(|runtime| runtime.resolve_function(self.spec)))
    }
}

macro_rules! impl_js_function_call {
    // Base case: zero arguments
    (0,) => {
        impl<R: BatchableResult + EncodeTypeDef + 'static> JsFunction<fn() -> R> {
            pub fn call(&self) -> R {
                run_js_sync::<R>(self.id(), |encoder| {
                    encode_function_types::<fn() -> R>(encoder);
                })
            }
        }
    };
    // Recursive case: N arguments
    ($n:expr, $($T:ident $arg:ident),+) => {
        impl<$($T: EncodeTypeDef,)+ R: BatchableResult + EncodeTypeDef + 'static>
            JsFunction<fn($($T),+) -> R>
        {
            pub fn call(&self, $($arg: $T),+) -> R
            where
                $($T: BinaryEncode,)+
            {
                run_js_sync::<R>(self.id(), |encoder| {
                    encode_function_types::<fn($($T),+) -> R>(encoder);
                    $($arg.encode(encoder);)+
                })
            }
        }
    };
}

impl_js_function_call!(0,);
impl_js_function_call!(1, T1 arg1);
impl_js_function_call!(2, T1 arg1, T2 arg2);
impl_js_function_call!(3, T1 arg1, T2 arg2, T3 arg3);
impl_js_function_call!(4, T1 arg1, T2 arg2, T3 arg3, T4 arg4);
impl_js_function_call!(5, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5);
impl_js_function_call!(6, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6);
impl_js_function_call!(7, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7);
impl_js_function_call!(8, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8);
impl_js_function_call!(9, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9);
impl_js_function_call!(10, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10);
impl_js_function_call!(11, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11);
impl_js_function_call!(12, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12);
impl_js_function_call!(13, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13);
impl_js_function_call!(14, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14);
impl_js_function_call!(15, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15);
impl_js_function_call!(16, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16);
impl_js_function_call!(17, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17);
impl_js_function_call!(18, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18);
impl_js_function_call!(19, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19);
impl_js_function_call!(20, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20);
impl_js_function_call!(21, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21);
impl_js_function_call!(22, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22);
impl_js_function_call!(23, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23);
impl_js_function_call!(24, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24);
impl_js_function_call!(25, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25);
impl_js_function_call!(26, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26);
impl_js_function_call!(27, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27);
impl_js_function_call!(28, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27, T28 arg28);
impl_js_function_call!(29, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27, T28 arg28, T29 arg29);
impl_js_function_call!(30, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27, T28 arg28, T29 arg29, T30 arg30);
impl_js_function_call!(31, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27, T28 arg28, T29 arg29, T30 arg30, T31 arg31);
impl_js_function_call!(32, T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16, T17 arg17, T18 arg18, T19 arg19, T20 arg20, T21 arg21, T22 arg22, T23 arg23, T24 arg24, T25 arg25, T26 arg26, T27 arg27, T28 arg28, T29 arg29, T30 arg30, T31 arg31, T32 arg32);
