//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{OnceLock, RwLock};

use slotmap::{DefaultKey, KeyData};
use winit::event_loop::EventLoopProxy;

use crate::encode::BinaryDecode;
use crate::function::{
    RustValue, DROP_NATIVE_REF_FN_ID, THREAD_LOCAL_FUNCTION_ENCODER,
};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub struct WryRuntime {
    pub proxy: EventLoopProxy<IPCMessage>,
    pub(crate) queued_rust_calls: RwLock<Vec<IPCMessage>>,
    pub(crate) sender: RwLock<Option<Sender<IPCMessage>>>,
}

impl WryRuntime {
    /// Create a new runtime with the given event loop proxy.
    pub fn new(proxy: EventLoopProxy<IPCMessage>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            sender: RwLock::new(None),
        }
    }

    /// Send a response back to JavaScript.
    pub fn js_response(&self, responder: IPCMessage) {
        let _ = self.proxy.send_event(responder);
    }

    /// Queue a Rust call from JavaScript.
    pub fn queue_rust_call(&self, responder: IPCMessage) {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            let _ = sender.send(responder);
        } else {
            self.queued_rust_calls.write().unwrap().push(responder);
        }
    }

    /// Set the sender for Rust calls and flush any queued calls.
    pub fn set_sender(&self, sender: Sender<IPCMessage>) {
        let mut queued = self.queued_rust_calls.write().unwrap();
        *self.sender.write().unwrap() = Some(sender);
        for call in queued.drain(..) {
            if let Some(sender) = self.sender.read().unwrap().as_ref() {
                let _ = sender.send(call);
            }
        }
    }
}

static RUNTIME: OnceLock<WryRuntime> = OnceLock::new();

/// Set the event loop proxy for the runtime.
///
/// This must be called once before any JS operations are performed.
pub fn set_event_loop_proxy(proxy: EventLoopProxy<IPCMessage>) {
    RUNTIME
        .set(WryRuntime::new(proxy))
        .unwrap_or_else(|_| panic!("Event loop proxy already set"));
}

/// Get the runtime environment.
///
/// Panics if the runtime has not been initialized.
pub fn get_runtime() -> &'static WryRuntime {
    RUNTIME.get().expect("Event loop proxy not set")
}

thread_local! {
    static THREAD_LOCAL_RECEIVER: Receiver<IPCMessage> = {
        let runtime = get_runtime();
        let (sender, receiver) = mpsc::channel();
        runtime.set_sender(sender);
        receiver
    };
}

/// Wait for a JS response, handling any Rust callbacks that occur during the wait.
pub fn wait_for_js_event<R: BinaryDecode>() -> R {
    let runtime = get_runtime();
    THREAD_LOCAL_RECEIVER.with(|receiver| {
        while let Ok(response) = receiver.recv() {
            let decoder = response.decoded().expect("Failed to decode response");
            match decoder {
                DecodedVariant::Respond { mut data } => {
                    return R::decode(&mut data).expect("Failed to decode return value");
                }
                DecodedVariant::Evaluate { mut data } => {
                    handle_rust_callback(runtime, &mut data);
                }
            }
        }
        panic!("Channel closed")
    })
}

/// Handle a Rust callback invocation from JavaScript.
fn handle_rust_callback(runtime: &WryRuntime, data: &mut DecodedData) {
    let fn_id = data.take_u32().expect("Failed to read fn_id");
    match fn_id {
        // Call a registered Rust callback
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
                    .downcast_mut::<RustValue>()
                    .expect("Failed to downcast to RustCallback");

                let response = IPCMessage::new_respond(|encoder| {
                    (function_callback.f)(data, encoder);
                });

                runtime.js_response(response);

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
        // Drop a native Rust object when JS GC'd the wrapper
        DROP_NATIVE_REF_FN_ID => {
            let key: DefaultKey = KeyData::from_ffi(data.take_u64().unwrap()).into();

            // Remove the object from the thread-local encoder
            THREAD_LOCAL_FUNCTION_ENCODER.with(|fn_encoder| {
                fn_encoder.borrow_mut().functions.remove(key);
            });

            // Send empty response
            let response = IPCMessage::new_respond(|_| {});
            runtime.js_response(response);
        }
        _ => todo!(),
    }
}
