//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::task::{Context, Poll};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures_core::stream::Stream;
use once_cell::sync::OnceCell;
use spin::RwLock;

use slotmap::{DefaultKey, KeyData};

use crate::encode::BinaryDecode;
use crate::function::{
    CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback, THREAD_LOCAL_OBJECT_ENCODER,
};
use crate::ipc::{DecodedData, DecodedVariant, EncodedData, IPCMessage};

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
    pub(crate) sender: RwLock<Option<UnboundedSender<IPCMessage>>>,
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
        if let Some(sender) = self.sender.write().as_mut() {
            let _ = sender.start_send(responder);
        } else {
            self.queued_rust_calls.write().push(responder);
        }
    }

    /// Set the sender for Rust calls and flush any queued calls.
    pub fn set_sender(&self, sender: UnboundedSender<IPCMessage>) {
        let mut queued = self.queued_rust_calls.write();
        *self.sender.write() = Some(sender);
        for call in queued.drain(..) {
            if let Some(sender) = self.sender.write().as_mut() {
                let _ = sender.start_send(call);
            }
        }
    }
}

static RUNTIME: OnceCell<WryRuntime> = OnceCell::new();

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
    static THREAD_LOCAL_RECEIVER: RefCell<UnboundedReceiver<IPCMessage>> = {
        let runtime = get_runtime();
        let (sender, receiver) = futures_channel::mpsc::unbounded();
        runtime.set_sender(sender);
        RefCell::new(receiver)
    };
}

/// Wait for a JS response, handling any Rust callbacks that occur during the wait.
pub async fn wait_for_js_result<R: BinaryDecode>() -> R {
    loop {
        if let Some(result) = wait_for_js_event::<R>().await {
            return result;
        }
    }
}

pub async fn wait_for_js_event<R: BinaryDecode>() -> Option<R> {
    progress_js_with(|mut data| R::decode(&mut data).expect("Failed to decode return value")).await
}

pub async fn progress_js_with<O>(with_respond: impl for<'a> Fn(DecodedData<'a>) -> O) -> Option<O> {
    let runtime = get_runtime();
    fn poll_fn(context: &mut Context) -> Poll<Option<IPCMessage>> {
        THREAD_LOCAL_RECEIVER.with(|receiver| {
            let mut receiver = receiver.borrow_mut();
            let receiver = &mut *receiver;
            let receiver = std::pin::pin!(receiver);
            receiver.poll_next(context)
        })
    }
    let response = std::future::poll_fn(move |cx| poll_fn(cx))
        .await
        .expect("Failed to receive JS response");

    let decoder = response.decoded().expect("Failed to decode response");
    match decoder {
        DecodedVariant::Respond { data } => Some(with_respond(data)),
        DecodedVariant::Evaluate { mut data } => {
            handle_rust_callback(runtime, &mut data);
            None
        }
    }
}

/// Handle a Rust callback invocation from JavaScript.
fn handle_rust_callback(runtime: &WryRuntime, data: &mut DecodedData) {
    let fn_id = data.take_u32().expect("Failed to read fn_id");
    match fn_id {
        // Call a registered Rust callback
        0 => {
            let key = KeyData::from_ffi(data.take_u64().unwrap()).into();

            // Clone the Rc while briefly borrowing the SlotMap, then release the borrow.
            // This allows nested callbacks to access the SlotMap during our callback execution.
            let callback = THREAD_LOCAL_OBJECT_ENCODER.with(|fn_encoder| {
                let encoder = fn_encoder.borrow();
                let function = encoder
                    .functions
                    .get(key)
                    .expect("Function not found for key");

                let rust_callback = function
                    .downcast_ref::<RustCallback>()
                    .expect("Failed to downcast to RustCallback");

                rust_callback.clone_rc()
            });
            // SlotMap borrow is now released - nested callbacks can access it

            // Push a borrow frame before calling the callback - nested calls won't clear our borrowed refs
            crate::batch::BATCH_STATE.with(|state| state.borrow_mut().push_borrow_frame());

            // Call through the cloned Rc (uniform Fn interface)
            let response = IPCMessage::new_respond(|encoder| {
                (callback)(data, encoder);
            });

            // Pop the borrow frame after the callback completes
            crate::batch::BATCH_STATE.with(|state| state.borrow_mut().pop_borrow_frame());

            // Send response to JS
            runtime.js_response(response);
        }
        // Drop a native Rust object when JS GC'd the wrapper
        DROP_NATIVE_REF_FN_ID => {
            let key: DefaultKey = KeyData::from_ffi(data.take_u64().unwrap()).into();

            // Remove the object from the thread-local encoder
            THREAD_LOCAL_OBJECT_ENCODER.with(|fn_encoder| {
                fn_encoder.borrow_mut().functions.remove(key);
            });

            // Send empty response
            let response = IPCMessage::new_respond(|_| {});
            runtime.js_response(response);
        }
        // Call an exported Rust struct method
        CALL_EXPORT_FN_ID => {
            // Read the export name
            let export_name =
                crate::encode::BinaryDecode::decode(data).expect("Failed to decode export name");
            let export_name: alloc::string::String = export_name;

            // Find the export handler
            let export = crate::inventory::iter::<crate::JsExportSpec>()
                .find(|e| e.name == export_name)
                .unwrap_or_else(|| panic!("Unknown export: {}", export_name));

            // Call the handler
            let result = (export.handler)(data);

            // Send response
            let response = match result {
                Ok(encoded) => IPCMessage::new_respond(|encoder| {
                    encoder.extend(&encoded);
                }),
                Err(err) => {
                    panic!("Export call failed: {}", err);
                }
            };
            runtime.js_response(response);
        }
        _ => todo!(),
    }
}
