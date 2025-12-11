use serde::de::DeserializeOwned;
use serde::Deserialize;
use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::sync::{OnceLock, RwLock, mpsc};
use winit::event_loop::EventLoopProxy;

use crate::DomEnv;
use crate::ipc::IPCMessage;

pub(crate) struct Encoder {
    functions: SlotMap<
        DefaultKey,
        Option<Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync>>,
    >,
}

impl Encoder {
    pub(crate) fn new() -> Self {
        Self {
            functions: SlotMap::new(),
        }
    }

    pub(crate) fn encode<T: RustEncode<P>, P>(&mut self, value: T) -> serde_json::Value {
        value.encode(self)
    }

    fn encode_function<T: IntoRustCallable<P>, P>(&mut self, function: T) -> serde_json::Value {
        let key = self.functions.insert(Some(function.into()));
        serde_json::json!({
            "type": "function",
            "id": key.data().as_ffi(),
        })
    }
}

pub(crate) trait RustEncode<P = ()> {
    fn encode(self, encoder: &mut Encoder) -> serde_json::Value;
}

impl RustEncode for String {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::String(self)
    }
}

impl RustEncode for () {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::Null
    }
}

impl RustEncode for i32 {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::Number(serde_json::Number::from(self))
    }
}

impl<F, P> RustEncode<P> for F
where
    F: IntoRustCallable<P>,
{
    fn encode(self, encoder: &mut Encoder) -> serde_json::Value {
        encoder.encode_function(self)
    }
}

trait IntoRustCallable<T> {
    fn into(self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync>;
}

impl<R, F> IntoRustCallable<fn() -> R> for F
where
    F: FnMut() -> R + Send + Sync + 'static,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |_: Vec<serde_json::Value>| {
            let result: R = (self)();
            serde_json::to_value(result).unwrap()
        })
    }
}

impl<T, R, F> IntoRustCallable<fn(T) -> R> for F
where
    F: FnMut(T) -> R + Send + Sync + 'static,
    T: for<'de> Deserialize<'de>,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |args: Vec<serde_json::Value>| {
            let mut args_iter = args.into_iter();
            let arg: T = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let result: R = (self)(arg);
            serde_json::to_value(result).unwrap()
        })
    }
}

impl<T1, T2, R, F> IntoRustCallable<fn(T1, T2) -> R> for F
where
    F: FnMut(T1, T2) -> R + Send + Sync + 'static,
    T1: for<'de> Deserialize<'de>,
    T2: for<'de> Deserialize<'de>,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |args: Vec<serde_json::Value>| {
            let mut args_iter = args.into_iter();
            let arg1: T1 = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let arg2: T2 = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let result: R = (self)(arg1, arg2);
            serde_json::to_value(result).unwrap()
        })
    }
}

pub(crate) struct JSFunction<T> {
    id: u64,
    function: PhantomData<T>,
}

impl<T> JSFunction<T> {
    pub const fn new(id: u64) -> Self {
        Self {
            id,
            function: PhantomData,
        }
    }
}

impl<T, R> JSFunction<fn(T) -> R> {
    pub fn call<P>(&self, args: T) -> R
    where
        T: RustEncode<P>,
        R: DeserializeOwned,
    {
        let args_json = encode_in_thread_local(args);
        run_js_sync(&get_dom().proxy, self.id, vec![args_json])
    }
}

impl<T1, T2, R> JSFunction<fn(T1, T2) -> R> {
    pub fn call<P1, P2>(&self, arg1: T1, arg2: T2) -> R
    where
        T1: RustEncode<P1>,
        T2: RustEncode<P2>,
        R: DeserializeOwned,
    {
        let arg1_json = encode_in_thread_local(arg1);
        let arg2_json = encode_in_thread_local(arg2);
        run_js_sync(&get_dom().proxy, self.id, vec![arg1_json, arg2_json])
    }
}
fn run_js_sync<T: DeserializeOwned>(
    proxy: &EventLoopProxy<IPCMessage>,
    fn_id: u64,
    args: Vec<serde_json::Value>,
) -> T {
    println!("Sending JS evaluate request...");
    _ = proxy.send_event(IPCMessage::Evaluate { fn_id, args });

    wait_for_js_event()
}

pub(crate) fn wait_for_js_event<T: DeserializeOwned>() -> T {
    let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
    THREAD_LOCAL_ENCODER.with(|tle| {
        println!("Waiting for JS response...");
        while let Ok(response) = tle.receiver.recv() {
            println!("Received response: {:?}", response);
            match response {
                IPCMessage::Respond { response } => {
                    println!("Got response from JS: {:?}", response);
                    return serde_json::from_value(response).unwrap();
                }
                IPCMessage::Evaluate { fn_id, args } => {
                    let key = KeyData::from_ffi(fn_id).into();
                    let function = {
                        let mut encoder = tle.encoder.write().unwrap();
                        encoder
                            .functions
                            .get_mut(key)
                            .map(|f| f.take().expect("function cannot be called recursively"))
                    };
                    if let Some(mut function) = function {
                        let result = function(args);
                        println!(
                            "Evaluated function in Rust, sending response back to JS: {:?}",
                            result
                        );
                        env.js_response(IPCMessage::Respond { response: result });
                        // Insert it back
                        let mut encoder = tle.encoder.write().unwrap();
                        encoder.functions.get_mut(key).unwrap().replace(function);
                    }
                }
                IPCMessage::Shutdown => {
                    panic!()
                }
            }
        }
        panic!()
    })
}

struct ThreadLocalEncoder {
    encoder: RwLock<Encoder>,
    receiver: Receiver<IPCMessage>,
}

thread_local! {
    static THREAD_LOCAL_ENCODER: ThreadLocalEncoder = ThreadLocalEncoder {
        encoder: RwLock::new(Encoder::new()),
        receiver: {
            let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
            let (sender, receiver) = mpsc::channel();
            env.set_sender(sender);
            receiver
        },
    };
}

fn encode_in_thread_local<T: RustEncode<P>, P>(value: T) -> serde_json::Value {
    println!("Encoding value in Rust...");
    THREAD_LOCAL_ENCODER.with(|tle| {
        println!("Encoding value in thread local...");
        let mut encoder = tle.encoder.write().unwrap();
        println!("Got encoder lock...");
        encoder.encode(value)
    })
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
