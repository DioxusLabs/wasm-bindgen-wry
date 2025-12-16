use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::sync::{OnceLock, mpsc};
use winit::event_loop::EventLoopProxy;

use crate::DomEnv;
use crate::ipc::{DecodedData, DecodedVariant, EncodedData, IPCMessage, MessageType};

/// A reference to a JavaScript heap object, identified by a unique ID.
/// References are encoded as u64 in the binary protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JSHeapRef {
    id: u64,
}

/// Trait for creating a JavaScript type instance.
/// Used to map Rust types to their JavaScript type constructors.
pub(crate) trait TypeConstructor<P = ()> {
    fn create_type_instance() -> String;
}

/// Trait for encoding Rust values into the binary protocol.
/// Each type specifies how to serialize itself.
pub(crate) trait BinaryEncode<P = ()> {
    fn encode(self, encoder: &mut EncodedData);
}

/// Trait for decoding values from the binary protocol.
/// Each type specifies how to deserialize itself.
pub(crate) trait BinaryDecode: Sized {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()>;
}

/// Trait for return types that can be used in batched JS calls.
/// Determines how the type behaves during batching.
pub trait BatchableResult: BinaryDecode + std::fmt::Debug{
    /// Whether this result type requires flushing the batch to get the actual value.
    /// Returns false for opaque types (placeholder) and trivial types (known value).
    fn needs_flush() -> bool;

    /// Get a placeholder/trivial value during batching.
    /// For opaque types, this reserves a heap ID from the batch.
    /// For trivial types like (), this returns the known value.
    /// For types that need_flush, this is never called.
    fn batched_placeholder(batch: &mut BatchState) -> Self;
}

impl BatchableResult for () {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(_: &mut BatchState) -> Self {}
}

/// Implement BatchableResult for types that always need a flush to get the result.
macro_rules! impl_needs_flush {
    ($($ty:ty),*) => {
        $(
            impl BatchableResult for $ty {
                fn needs_flush() -> bool {
                    true
                }

                fn batched_placeholder(_batch: &mut BatchState) -> Self {
                    unreachable!("needs_flush types should never call batched_placeholder")
                }
            }
        )*
    };
}

impl_needs_flush!(bool, u8, u16, u32, String);

impl TypeConstructor for () {
    fn create_type_instance() -> String {
        "new window.NullType()".to_string()
    }
}

impl BinaryEncode for () {
    fn encode(self, _encoder: &mut EncodedData) {
        // Unit type encodes as nothing
    }
}

impl BinaryDecode for () {
    fn decode(_decoder: &mut DecodedData) -> Result<Self, ()> {
        Ok(())
    }
}

impl TypeConstructor for bool {
    fn create_type_instance() -> String {
        "new window.BoolType()".to_string()
    }
}

impl BinaryEncode for bool {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u8(if self { 1 } else { 0 });
    }
}

impl BinaryDecode for bool {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        Ok(decoder.take_u8()? != 0)
    }
}

impl TypeConstructor for u8 {
    fn create_type_instance() -> String {
        "window.U8Type".to_string()
    }
}

impl BinaryEncode for u8 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u8(self);
    }
}

impl BinaryDecode for u8 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        decoder.take_u8()
    }
}

impl TypeConstructor for u16 {
    fn create_type_instance() -> String {
        "window.U16Type".to_string()
    }
}

impl BinaryEncode for u16 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u16(self);
    }
}

impl BinaryDecode for u16 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        decoder.take_u16()
    }
}

impl TypeConstructor for u32 {
    fn create_type_instance() -> String {
        "window.U32Type".to_string()
    }
}

impl BinaryEncode for u32 {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u32(self);
    }
}

impl BinaryDecode for u32 {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        decoder.take_u32()
    }
}

impl TypeConstructor for str {
    fn create_type_instance() -> String {
        "window.strType".to_string()
    }
}

impl BinaryEncode for &str {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(self);
    }
}

impl TypeConstructor for String {
    fn create_type_instance() -> String {
        "window.strType".to_string()
    }
}

impl BinaryEncode for String {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(&self);
    }
}

impl BinaryDecode for String {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        Ok(decoder.take_str()?.to_string())
    }
}

pub struct RustCallbackMarker<P> {
    phantom: PhantomData<P>,
}

struct RustCallback {
    f: Box<dyn FnMut(&mut DecodedData, &mut EncodedData)>,
}

impl RustCallback {
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut(&mut DecodedData, &mut EncodedData) + 'static,
    {
        Self { f: Box::new(f) }
    }
}

impl<R: BinaryEncode<P>, P, F> BinaryEncode<RustCallbackMarker<(P,)>> for F
where
    F: FnMut() -> R + 'static,
{
    fn encode(mut self, encoder: &mut EncodedData) {
        let value = register_callback(RustCallback::new(
            move |_: &mut DecodedData, encoder: &mut EncodedData| {
                let result = (self)();
                result.encode(encoder);
            },
        ));

        encoder.push_u64(value.data().as_ffi());
    }
}

impl TypeConstructor for JSHeapRef {
    fn create_type_instance() -> String {
        "new window.HeapRefType()".to_string()
    }
}

impl BinaryEncode for JSHeapRef {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_u64(self.id);
    }
}

impl BinaryDecode for JSHeapRef {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        Ok(JSHeapRef {
            id: decoder.take_u64()?,
        })
    }
}

impl BatchableResult for JSHeapRef {
    fn needs_flush() -> bool {
        false
    }

    fn batched_placeholder(batch: &mut BatchState) -> Self {
        JSHeapRef {
            id: batch.get_next_heap_id(),
        }
    }
}

impl<T: TypeConstructor<P>, P> TypeConstructor<P> for Option<T> {
    fn create_type_instance() -> String {
        format!("new window.OptionType({})", T::create_type_instance())
    }
}

impl<R: TypeConstructor<P>, P, F> TypeConstructor<RustCallbackMarker<(P,)>> for F
where
    F: FnMut() -> R + 'static,
{
    fn create_type_instance() -> String {
        format!("new window.CallbackType({})", R::create_type_instance())
    }
}

impl<T: BinaryDecode> BinaryDecode for Option<T> {
    fn decode(decoder: &mut DecodedData) -> Result<Self, ()> {
        let has_value = decoder.take_u8()? != 0;
        if has_value {
            Ok(Some(T::decode(decoder)?))
        } else {
            Ok(None)
        }
    }
}

/// Encoder for storing Rust objects that can be referenced called from JS.
pub(crate) struct ObjEncoder {
    functions: SlotMap<DefaultKey, Option<Box<dyn Any>>>,
}

impl ObjEncoder {
    pub(crate) fn new() -> Self {
        Self {
            functions: SlotMap::new(),
        }
    }

    fn register_value<T: 'static>(&mut self, value: T) -> DefaultKey {
        self.functions.insert(Some(Box::new(value)))
    }
}

thread_local! {
    static THREAD_LOCAL_FUNCTION_ENCODER: RefCell<ObjEncoder> = RefCell::new(ObjEncoder::new());
}

/// Register a callback with the thread-local encoder using a short borrow
fn register_callback(callback: RustCallback) -> DefaultKey {
    THREAD_LOCAL_FUNCTION_ENCODER
        .with(|fn_encoder| fn_encoder.borrow_mut().register_value(callback))
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

/// Core function for executing JavaScript calls.
///
/// For each call:
/// 1. Encode the current evaluate message into the current batch
/// 2. If the return value is needed immediately, flush the batch and return the result
/// 3. Otherwise get the pending result from BatchableResult
fn run_js_sync<R: BatchableResult>(fn_id: u32, add_args: impl FnOnce(&mut EncodedData)) -> R {
    // Step 1: Encode the operation into the batch and get placeholder for non-flush types
    // IMPORTANT: We call batched_placeholder for ALL non-flush types
    // because it tracks opaque allocations for heap ID synchronization
    let placeholder = BATCH_STATE.with(|state| {
        let mut batch = state.borrow_mut();
        batch.add_operation(fn_id, add_args);

        // Get placeholder for types that don't need flush
        // This also increments opaque_count to keep heap IDs in sync
        if !R::needs_flush() {
            Some(R::batched_placeholder(&mut batch))
        } else {
            None
        }
    });

    // Step 2: If return value needed immediately OR not in batch mode, flush and return
    if R::needs_flush() || !is_batching() {
        return flush_and_return::<R>();
    }

    // Step 3: Return the placeholder (only reached in batch mode for non-flush types)
    placeholder.expect("Placeholder should exist for non-flush types in batch mode")
}

/// Flush the current batch and return the decoded result.
fn flush_and_return<R: BinaryDecode>() -> R {
    let batch_msg = BATCH_STATE.with(|state| state.borrow_mut().take_message());

    // Send and wait for result
    let proxy = &get_dom().proxy;
    let _ = proxy.send_event(batch_msg);
    let result: R = wait_for_js_event();

    result
}

/// Wait for a JS response, handling any Rust callbacks that occur during the wait.
pub(crate) fn wait_for_js_event<R: BinaryDecode>() -> R {
    let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
    THREAD_LOCAL_RECEIVER.with(|receiver| {
        while let Ok(response) = receiver.recv() {
            let decoder = response.decoded().expect("Failed to decode response");
            match decoder {
                DecodedVariant::Respond { mut data } => {
                    return R::decode(&mut data).expect("Failed to decode return value");
                }
                DecodedVariant::Evaluate { mut data } => {
                    handle_rust_callback(env, &mut data);
                }
            }
        }
        panic!("Channel closed")
    })
}

/// Handle a Rust callback invocation from JavaScript.
fn handle_rust_callback(env: &DomEnv, data: &mut DecodedData) {
    let fn_id = data.take_u32().expect("Failed to read fn_id");
    match fn_id {
        0 => {
            let key = KeyData::from_ffi(data.take_u64().unwrap()).into();

            // Get the function from the thread-local encoder
            let function = THREAD_LOCAL_FUNCTION_ENCODER.with(|fn_encoder| {
                fn_encoder
                    .borrow_mut()
                    .functions
                    .get_mut(key)
                    .and_then(|f| f.take())
            });

            if let Some(mut function) = function {
                // Downcast to RustCallback and call it
                let function_callback = function
                    .downcast_mut::<RustCallback>()
                    .expect("Failed to downcast to RustCallback");

                let response = IPCMessage::new_respond(|encoder| {
                    (function_callback.f)(data, encoder);
                });

                env.js_response(response);

                // Insert it back into the thread-local encoder
                THREAD_LOCAL_FUNCTION_ENCODER.with(|fn_encoder| {
                    fn_encoder
                        .borrow_mut()
                        .functions
                        .get_mut(key)
                        .unwrap()
                        .replace(function);
                });
            }
        }
        _ => todo!(),
    }
}

thread_local! {
    static THREAD_LOCAL_RECEIVER: Receiver<IPCMessage> = {
        let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
        let (sender, receiver) = mpsc::channel();
        env.set_sender(sender);
        receiver
    };
}

static EVENT_LOOP_PROXY: OnceLock<DomEnv> = OnceLock::new();

pub(crate) fn set_event_loop_proxy(proxy: EventLoopProxy<IPCMessage>) {
    EVENT_LOOP_PROXY
        .set(DomEnv::new(proxy))
        .unwrap_or_else(|_| panic!("Event loop proxy already set"));
}

pub(crate) fn get_dom() -> &'static DomEnv {
    EVENT_LOOP_PROXY.get().expect("Event loop proxy not set")
}

/// State for batching operations.
/// Every evaluation is a batch - it may just have one operation.
pub struct BatchState {
    /// The encoder accumulating batched operations
    encoder: EncodedData,
    /// Track the next expected heap ID for placeholder allocation
    next_heap_id: u64,
    /// Whether we're inside a batch() call
    is_batching: bool,
}

impl BatchState {
    fn new() -> Self {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Evaluate as u8);
        Self {
            encoder,
            next_heap_id: 0,
            is_batching: false,
        }
    }

    /// Get the next heap ID for placeholder allocation
    fn get_next_heap_id(&mut self) -> u64 {
        let id = self.next_heap_id;
        self.next_heap_id += 1;
        id
    }

    fn add_operation(&mut self, fn_id: u32, add_args: impl FnOnce(&mut EncodedData)) {
        self.encoder.push_u32(fn_id);
        add_args(&mut self.encoder);
    }

    /// Take the message data and reset the batch for reuse
    fn take_message(&mut self) -> IPCMessage {
        let msg = IPCMessage::new(self.encoder.to_bytes());

        // Reset for next batch
        self.encoder = EncodedData::new();
        self.encoder.push_u8(MessageType::Evaluate as u8);

        msg
    }
    
    fn is_empty(&self) -> bool {
        // 12 bytes for offsets + 1 byte for message type
        self.encoder.byte_len() <= 13
    }
}

thread_local! {
    /// Thread-local batch state - always exists, reset after each flush
    static BATCH_STATE: RefCell<BatchState> = RefCell::new(BatchState::new());
}

/// Check if we're currently inside a batch() call
fn is_batching() -> bool {
    BATCH_STATE.with(|state| state.borrow().is_batching)
}

/// Execute operations inside a batch. Operations that return opaque types (like JSHeapRef)
/// will be batched and executed together. Operations that return non-opaque types will
/// flush the batch to get the actual result.
pub fn batch<R, F: FnOnce() -> R>(f: F) -> R {
    // Start batching
    BATCH_STATE.with(|state| state.borrow_mut().is_batching = true);

    // Execute the closure
    let result = f();

    // Flush any remaining batched operations
    let has_pending = BATCH_STATE.with(|state| !state.borrow().is_empty());
    if has_pending {
        flush_and_return::<()>();
    }

    // End batching
    BATCH_STATE.with(|state| state.borrow_mut().is_batching = false);

    result
}

