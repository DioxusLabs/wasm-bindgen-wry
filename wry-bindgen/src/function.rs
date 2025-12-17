//! JavaScript function references and Rust callback management.
//!
//! This module provides types for calling JavaScript functions from Rust
//! and for registering Rust callbacks that can be called from JavaScript.

#[cfg(feature = "runtime")]
use std::any::Any;
#[cfg(feature = "runtime")]
use std::cell::RefCell;
use std::marker::PhantomData;

#[cfg(feature = "runtime")]
use slotmap::{DefaultKey, SlotMap};

use crate::batch::run_js_sync;
use crate::encode::{BatchableResult, BinaryEncode};
#[cfg(feature = "runtime")]
use crate::ipc::{DecodedData, EncodedData};

/// Reserved function ID for dropping native Rust refs when JS objects are GC'd.
/// JS sends this when a FinalizationRegistry callback fires for a RustFunction.
pub const DROP_NATIVE_REF_FN_ID: u32 = 0xFFFFFFFF;

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

impl<R: BatchableResult> JSFunction<fn() -> R> {
    pub fn call(&self) -> R {
        run_js_sync::<R>(self.id, |_| {})
    }
}

impl<T1, R: BatchableResult> JSFunction<fn(T1) -> R> {
    pub fn call<P1>(&self, arg: T1) -> R
    where
        T1: BinaryEncode<P1>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg.encode(encoder);
        })
    }
}

impl<T1, T2, R: BatchableResult> JSFunction<fn(T1, T2) -> R> {
    pub fn call<P1, P2>(&self, arg1: T1, arg2: T2) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
        })
    }
}

impl<T1, T2, T3, R: BatchableResult> JSFunction<fn(T1, T2, T3) -> R> {
    pub fn call<P1, P2, P3>(&self, arg1: T1, arg2: T2, arg3: T3) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, R: BatchableResult> JSFunction<fn(T1, T2, T3, T4) -> R> {
    pub fn call<P1, P2, P3, P4>(&self, arg1: T1, arg2: T2, arg3: T3, arg4: T4) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, R: BatchableResult> JSFunction<fn(T1, T2, T3, T4, T5) -> R> {
    pub fn call<P1, P2, P3, P4, P5>(&self, arg1: T1, arg2: T2, arg3: T3, arg4: T4, arg5: T5) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, R: BatchableResult> JSFunction<fn(T1, T2, T3, T4, T5, T6) -> R> {
    pub fn call<P1, P2, P3, P4, P5, P6>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, T8, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7, T8) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7, P8>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
        arg8: T8,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
        T8: BinaryEncode<P8>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
            arg8.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, T8, T9, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7, T8, T9) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7, P8, P9>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
        arg8: T8,
        arg9: T9,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
        T8: BinaryEncode<P8>,
        T9: BinaryEncode<P9>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
            arg8.encode(encoder);
            arg9.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7, P8, P9, P10>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
        arg8: T8,
        arg9: T9,
        arg10: T10,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
        T8: BinaryEncode<P8>,
        T9: BinaryEncode<P9>,
        T10: BinaryEncode<P10>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
            arg8.encode(encoder);
            arg9.encode(encoder);
            arg10.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
        arg8: T8,
        arg9: T9,
        arg10: T10,
        arg11: T11,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
        T8: BinaryEncode<P8>,
        T9: BinaryEncode<P9>,
        T10: BinaryEncode<P10>,
        T11: BinaryEncode<P11>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
            arg8.encode(encoder);
            arg9.encode(encoder);
            arg10.encode(encoder);
            arg11.encode(encoder);
        })
    }
}

impl<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, R: BatchableResult>
    JSFunction<fn(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12) -> R>
{
    pub fn call<P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12>(
        &self,
        arg1: T1,
        arg2: T2,
        arg3: T3,
        arg4: T4,
        arg5: T5,
        arg6: T6,
        arg7: T7,
        arg8: T8,
        arg9: T9,
        arg10: T10,
        arg11: T11,
        arg12: T12,
    ) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
        T4: BinaryEncode<P4>,
        T5: BinaryEncode<P5>,
        T6: BinaryEncode<P6>,
        T7: BinaryEncode<P7>,
        T8: BinaryEncode<P8>,
        T9: BinaryEncode<P9>,
        T10: BinaryEncode<P10>,
        T11: BinaryEncode<P11>,
        T12: BinaryEncode<P12>,
    {
        run_js_sync::<R>(self.id, |encoder| {
            arg1.encode(encoder);
            arg2.encode(encoder);
            arg3.encode(encoder);
            arg4.encode(encoder);
            arg5.encode(encoder);
            arg6.encode(encoder);
            arg7.encode(encoder);
            arg8.encode(encoder);
            arg9.encode(encoder);
            arg10.encode(encoder);
            arg11.encode(encoder);
            arg12.encode(encoder);
        })
    }
}

/// Internal type for storing Rust callback functions.
#[cfg(feature = "runtime")]
pub(crate) struct RustValue {
    pub(crate) f: Box<dyn FnMut(&mut DecodedData, &mut EncodedData)>,
}

#[cfg(feature = "runtime")]
impl RustValue {
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut(&mut DecodedData, &mut EncodedData) + 'static,
    {
        Self { f: Box::new(f) }
    }
}

/// Encoder for storing Rust objects that can be called from JS.
#[cfg(feature = "runtime")]
pub(crate) struct ObjEncoder {
    pub(crate) functions: SlotMap<DefaultKey, Option<Box<dyn Any>>>,
}

#[cfg(feature = "runtime")]
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

#[cfg(feature = "runtime")]
thread_local! {
    pub(crate) static THREAD_LOCAL_FUNCTION_ENCODER: RefCell<ObjEncoder> = RefCell::new(ObjEncoder::new());
}

/// Register a callback with the thread-local encoder using a short borrow
#[cfg(feature = "runtime")]
pub(crate) fn register_value(callback: RustValue) -> DefaultKey {
    THREAD_LOCAL_FUNCTION_ENCODER
        .with(|fn_encoder| fn_encoder.borrow_mut().register_value(callback))
}
