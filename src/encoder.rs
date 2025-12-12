use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::sync::{OnceLock, mpsc};
use winit::event_loop::EventLoopProxy;

use crate::DomEnv;
use crate::ipc::{DecodedData, EncodedData, IPCMessage};

/// A reference to a JavaScript heap object, identified by a unique ID.
/// References are encoded as u64 in the binary protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JSHeapRef {
    id: u64,
}

impl JSHeapRef {
    /// Get the raw ID of this heap reference
    pub fn id(&self) -> u64 {
        self.id
    }
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

impl BinaryEncode for &str {
    fn encode(self, encoder: &mut EncodedData) {
        encoder.push_str(self);
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

// impl<T: BinaryDecode, R: BinaryEncode<P>, P, F> BinaryEncode<RustCallbackMarker<(P,P,)>> for F where F: Fn(T) -> R + 'static {
//     fn encode(self, _: &mut EncodedData) {
//         register_callback(RustCallback::new(move |decoder: &mut DecodedData, encoder: &mut EncodedData| {
//             let arg1 = T::decode(decoder).expect("Failed to decode argument");
//             let result = (self)(arg1);
//             result.encode(encoder);
//         }));
//     }
// }

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
        let id = self.functions.insert(Some(Box::new(value)));
        println!("Registering Rust function in ObjEncoder with id: {:?}", id);
        id
    }
}

thread_local! {
    static THREAD_LOCAL_FUNCTION_ENCODER: RefCell<ObjEncoder> = RefCell::new(ObjEncoder::new());
}

/// Register a callback with the thread-local encoder using a short borrow
fn register_callback(callback: RustCallback) -> DefaultKey {
    THREAD_LOCAL_FUNCTION_ENCODER.with(|fn_encoder| {
        fn_encoder.borrow_mut().register_value(callback)
    })
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

impl<R: BinaryDecode> JSFunction<fn() -> R> {
    pub fn call(&self, _: ()) -> R {
        let encoder = EncodedData::new();
        run_js_sync::<R>(self.id, encoder)
    }
}

impl<T1, R: BinaryDecode> JSFunction<fn(T1) -> R> {
    pub fn call<P1>(&self, arg: T1) -> R
    where
        T1: BinaryEncode<P1>,
    {
        let mut encoder = EncodedData::new();
        arg.encode(&mut encoder);
        run_js_sync::<R>(self.id, encoder)
    }
}

impl<T1, T2, R: BinaryDecode> JSFunction<fn(T1, T2) -> R> {
    pub fn call<P1, P2>(&self, arg1: T1, arg2: T2) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
    {
        let mut encoder = EncodedData::new();
        arg1.encode(&mut encoder);
        arg2.encode(&mut encoder);
        run_js_sync::<R>(self.id, encoder)
    }
}

impl<T1, T2, T3, R: BinaryDecode> JSFunction<fn(T1, T2, T3) -> R> {
    pub fn call<P1, P2, P3>(&self, arg1: T1, arg2: T2, arg3: T3) -> R
    where
        T1: BinaryEncode<P1>,
        T2: BinaryEncode<P2>,
        T3: BinaryEncode<P3>,
    {
        let mut encoder = EncodedData::new();
        arg1.encode(&mut encoder);
        arg2.encode(&mut encoder);
        arg3.encode(&mut encoder);
        run_js_sync::<R>(self.id, encoder)
    }
}

fn run_js_sync<R: BinaryDecode>(fn_id: u32, args: EncodedData) -> R {
    let proxy = &get_dom().proxy;
    let data = crate::ipc::encode_evaluate(fn_id, &args);

    println!("Sending JS evaluate request for fn_id: {}", fn_id);
    _ = proxy.send_event(IPCMessage::Evaluate { fn_id, data });

    wait_for_js_event()
}

pub(crate) fn wait_for_js_event<R: BinaryDecode>() -> R {
    let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
    THREAD_LOCAL_RECEIVER.with(|receiver| {
        println!("Waiting for JS response...");
        while let Ok(response) = receiver.recv() {
            println!("Received response: {:?}", response);
            match response {
                IPCMessage::Respond { data } => {
                    println!("Got response from JS");
                    // Skip the message type (first u32 after header)
                    let mut decoder =
                        DecodedData::from_bytes(&data).expect("Failed to decode response");
                    let _ = decoder.take_u32(); // Skip message type
                    return R::decode(&mut decoder).expect("Failed to decode return value");
                }
                IPCMessage::Evaluate { fn_id, data } => {
                    println!("Received JS evaluate request for fn_id: {}", fn_id);
                    match fn_id {
                        0 => {
                            let mut decoder =
                                DecodedData::from_bytes(&data).expect("Failed to decode");
                            let _ = decoder.take_u32(); // Skip message type
                            let _ = decoder.take_u32(); // Skip fn_id
                            let key = KeyData::from_ffi(decoder.take_u64().unwrap() as u64).into();
                            println!("Decoded function key: {:?}", key);

                            // Get the function from the thread-local encoder
                            let function = THREAD_LOCAL_FUNCTION_ENCODER.with(|fn_encoder| {
                                println!("within THREAD_LOCAL_FUNCTION_ENCODER");
                                fn_encoder
                                    .borrow_mut()
                                    .functions
                                    .get_mut(key)
                                    .and_then(|f| f.take())
                            });
                            println!("Got function from encoder: {:?}", function);

                            if let Some(mut function) = function {
                                // Downcast to RustCallback and call it
                                let function_callback = function
                                    .downcast_mut::<RustCallback>()
                                    .expect("Failed to downcast to RustCallback");

                                let mut encoder = EncodedData::new();
                                encoder.push_u32(1); // Respond message type
                                println!("Calling Rust function from JS...");
                                (&mut function_callback.f)(
                                    &mut decoder,
                                    &mut encoder,
                                );
                                println!("Evaluated function in Rust, sending response back to JS");
                                env.js_response(IPCMessage::Respond {
                                    data: encoder.to_bytes(),
                                });

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
                IPCMessage::Shutdown => {
                    panic!("Shutdown received")
                }
            }
        }
        panic!("Channel closed")
    })
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
