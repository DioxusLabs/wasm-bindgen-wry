//! JavaScript function references and Rust callback management.
//!
//! This module provides types for calling JavaScript functions from Rust
//! and for registering Rust callbacks that can be called from JavaScript.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::{Cell, RefCell};
use core::marker::PhantomData;

use slotmap::{DefaultKey, SlotMap};

use crate::batch::run_js_sync;
use crate::encode::{BatchableResult, BinaryEncode, EncodeTypeDef, TYPE_CACHED, TYPE_FULL};
use crate::ipc::DecodedData;
use crate::ipc::EncodedData;

/// Reserved function ID for dropping native Rust refs when JS objects are GC'd.
/// JS sends this when a FinalizationRegistry callback fires for a RustFunction.
pub const DROP_NATIVE_REF_FN_ID: u32 = 0xFFFFFFFF;

/// Reserved function ID for calling exported Rust struct methods from JS.
/// JS sends this with the export name to call the appropriate handler.
pub const CALL_EXPORT_FN_ID: u32 = 0xFFFFFFFE;

thread_local! {
    /// Cache mapping type definition bytes to the assigned type_id for the JS side
    static TYPE_CACHE: RefCell<BTreeMap<Vec<u8>, u32>> = RefCell::new(BTreeMap::new());
    /// Next type ID to assign
    static NEXT_TYPE_ID: Cell<u32> = Cell::new(0);
}

/// Encode type definitions for a function call.
/// On first call for a type signature, sends TYPE_FULL + id + param_count + type defs.
/// On subsequent calls, sends TYPE_CACHED + id.
fn encode_function_types(encoder: &mut EncodedData, encode_types: impl FnOnce(&mut Vec<u8>)) {
    // Always encode type definitions to get the bytes
    let mut type_buf = Vec::new();
    encode_types(&mut type_buf);

    TYPE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(&id) = cache.get(&type_buf) {
            // Cached - just send marker + ID
            encoder.push_u8(TYPE_CACHED);
            encoder.push_u32(id);
        } else {
            // First time - send full type def + ID
            let id = NEXT_TYPE_ID.with(|n| {
                let id = n.get();
                n.set(id + 1);
                id
            });
            cache.insert(type_buf.clone(), id);

            encoder.push_u8(TYPE_FULL);
            encoder.push_u32(id);

            // Push the type definition bytes
            for byte in type_buf {
                encoder.push_u8(byte);
            }
        }
    });
}

/// A reference to a JavaScript function that can be called from Rust.
///
/// The type parameter encodes the function signature.
/// Arguments and return values are serialized using the binary protocol.
pub struct JSFunction<T> {
    id: u32,
    function: PhantomData<T>,
}

impl<T> JSFunction<T> {
    pub const fn new(id: u32) -> Self {
        Self {
            id,
            function: PhantomData,
        }
    }

    /// Get the function ID.
    pub fn id(&self) -> u32 {
        self.id
    }
}

macro_rules! impl_js_function_call {
    // Base case: zero arguments
    (0,) => {
        impl<R: BatchableResult + EncodeTypeDef> JSFunction<fn() -> R> {
            pub fn call(&self) -> R {
                run_js_sync::<R>(self.id, |encoder| {
                    encode_function_types(encoder, |buf| {
                        buf.push(0);
                        R::encode_type_def(buf);
                    });
                })
            }
        }
    };
    // Recursive case: N arguments
    ($n:expr, $($T:ident $P:ident $arg:ident),+) => {
        impl<$($T: EncodeTypeDef,)+ R: BatchableResult + EncodeTypeDef>
            JSFunction<fn($($T),+) -> R>
        {
            pub fn call<$($P),+>(&self, $($arg: $T),+) -> R
            where
                $($T: BinaryEncode<$P>,)+
            {
                run_js_sync::<R>(self.id, |encoder| {
                    encode_function_types(encoder, |buf| {
                        buf.push($n);
                        $($T::encode_type_def(buf);)+
                        R::encode_type_def(buf);
                    });
                    $($arg.encode(encoder);)+
                })
            }
        }
    };
}

impl_js_function_call!(0,);
impl_js_function_call!(1, T1 P1 arg1);
impl_js_function_call!(2, T1 P1 arg1, T2 P2 arg2);
impl_js_function_call!(3, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3);
impl_js_function_call!(4, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4);
impl_js_function_call!(5, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5);
impl_js_function_call!(6, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6);
impl_js_function_call!(7, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7);
impl_js_function_call!(8, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8);
impl_js_function_call!(9, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9);
impl_js_function_call!(10, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10);
impl_js_function_call!(11, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11);
impl_js_function_call!(12, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12);
impl_js_function_call!(13, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13);
impl_js_function_call!(14, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14);
impl_js_function_call!(15, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15);
impl_js_function_call!(16, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16);
impl_js_function_call!(17, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17);
impl_js_function_call!(18, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18);
impl_js_function_call!(19, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19);
impl_js_function_call!(20, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20);
impl_js_function_call!(21, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21);
impl_js_function_call!(22, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22);
impl_js_function_call!(23, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23);
impl_js_function_call!(24, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24);
impl_js_function_call!(25, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25);
impl_js_function_call!(26, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26);
impl_js_function_call!(27, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27);
impl_js_function_call!(28, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27, T28 P28 arg28);
impl_js_function_call!(29, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27, T28 P28 arg28, T29 P29 arg29);
impl_js_function_call!(30, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27, T28 P28 arg28, T29 P29 arg29, T30 P30 arg30);
impl_js_function_call!(31, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27, T28 P28 arg28, T29 P29 arg29, T30 P30 arg30, T31 P31 arg31);
impl_js_function_call!(32, T1 P1 arg1, T2 P2 arg2, T3 P3 arg3, T4 P4 arg4, T5 P5 arg5, T6 P6 arg6, T7 P7 arg7, T8 P8 arg8, T9 P9 arg9, T10 P10 arg10, T11 P11 arg11, T12 P12 arg12, T13 P13 arg13, T14 P14 arg14, T15 P15 arg15, T16 P16 arg16, T17 P17 arg17, T18 P18 arg18, T19 P19 arg19, T20 P20 arg20, T21 P21 arg21, T22 P22 arg22, T23 P23 arg23, T24 P24 arg24, T25 P25 arg25, T26 P26 arg26, T27 P27 arg27, T28 P28 arg28, T29 P29 arg29, T30 P30 arg30, T31 P31 arg31, T32 P32 arg32);

/// Internal type for storing Rust callback functions.
pub(crate) struct RustCallback {
    pub(crate) f: Box<dyn FnMut(&mut DecodedData, &mut EncodedData)>,
}

impl RustCallback {
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut(&mut DecodedData, &mut EncodedData) + 'static,
    {
        Self { f: Box::new(f) }
    }
}

/// Encoder for storing Rust objects that can be called from JS.
pub(crate) struct ObjEncoder {
    pub(crate) functions: SlotMap<DefaultKey, Option<Box<dyn Any>>>,
}

impl ObjEncoder {
    pub(crate) fn new() -> Self {
        Self {
            functions: SlotMap::new(),
        }
    }

    pub(crate) fn register_value<T: 'static>(&mut self, value: T) -> DefaultKey {
        self.functions.insert(Some(Box::new(value)))
    }
}

thread_local! {
    pub(crate) static THREAD_LOCAL_FUNCTION_ENCODER: RefCell<ObjEncoder> = RefCell::new(ObjEncoder::new());
}

/// Register a callback with the thread-local encoder using a short borrow
pub(crate) fn register_value(callback: RustCallback) -> DefaultKey {
    THREAD_LOCAL_FUNCTION_ENCODER
        .with(|fn_encoder| fn_encoder.borrow_mut().register_value(callback))
}
