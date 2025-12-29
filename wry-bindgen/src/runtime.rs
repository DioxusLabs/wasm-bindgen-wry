//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{OnceLock, RwLock};

use slotmap::{DefaultKey, KeyData};

use crate::encode::BinaryDecode;
use crate::function::{DROP_NATIVE_REF_FN_ID, RustCallback, THREAD_LOCAL_FUNCTION_ENCODER};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};

/// Application-level events that can be sent through the event loop.
///
/// This enum wraps both IPC messages from JavaScript and control messages
/// from the application (like shutdown requests).
#[derive(Debug)]
pub enum AppEvent {
    /// An IPC message from JavaScript
    Ipc(IPCMessage),
    /// The webview has finished loading
    WebviewLoaded,
    /// Request to shut down the application with a status code
    Shutdown(i32),
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub struct WryRuntime {
    pub proxy: Box<dyn Fn(AppEvent) + Send + Sync>,
    pub(crate) queued_rust_calls: RwLock<Vec<IPCMessage>>,
    pub(crate) sender: RwLock<Option<Sender<IPCMessage>>>,
}

impl WryRuntime {
    /// Create a new runtime with the given event loop proxy.
    pub fn new(proxy: Box<dyn Fn(AppEvent) + Send + Sync>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            sender: RwLock::new(None),
        }
    }

    /// Send a response back to JavaScript.
    pub fn js_response(&self, responder: IPCMessage) {
        (self.proxy)(AppEvent::Ipc(responder));
    }

    /// Request the application to shut down with a status code.
    pub fn shutdown(&self, status: i32) {
        (self.proxy)(AppEvent::Shutdown(status));
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
pub fn set_event_loop_proxy(proxy: Box<dyn Fn(AppEvent) + Send + Sync>) {
    RUNTIME
        .set(WryRuntime::new(proxy))
        .unwrap_or_else(|_| panic!("Event loop proxy already set"));
}

/// Request the application to shut down with a status code.
///
/// This sends a shutdown event through the event loop, which will cause
/// the webview to close and the application to exit with the given status code.
pub fn shutdown(status: i32) {
    get_runtime().shutdown(status);
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
pub fn wait_for_js_result<R: BinaryDecode>() -> R {
    loop {
        if let Some(result) = wait_for_js_event::<R>() {
            return result;
        }
    }
}

pub fn wait_for_js_event<R: BinaryDecode>() -> Option<R> {
    let runtime = get_runtime();
    THREAD_LOCAL_RECEIVER.with(|receiver| {
        if let Ok(response) = receiver.recv() {
            let decoder = response.decoded().expect("Failed to decode response");
            match decoder {
                DecodedVariant::Respond { mut data } => {
                    return Some(R::decode(&mut data).expect("Failed to decode return value"));
                }
                DecodedVariant::Evaluate { mut data } => {
                    handle_rust_callback(runtime, &mut data);
                }
            }
        }
        None
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
                    .downcast_mut::<RustCallback>()
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
            } else {
                panic!("Function not found for key: {:?}", key);
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
