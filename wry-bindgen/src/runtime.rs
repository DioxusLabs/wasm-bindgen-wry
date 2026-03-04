//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use std::sync::Arc;
use std::sync::OnceLock;
use std::thread::Thread;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use spin::Mutex;

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};
use crate::object_store::ObjectHandle;
use crate::object_store::remove_object;

/// Application-level events that can be sent through the event loop.
///
/// This enum wraps both IPC messages from JavaScript and control messages
/// from the application (like shutdown requests).
#[derive(Debug, Clone)]
pub struct WryBindgenEvent {
    id: u64,
    event: AppEventVariant,
}

impl WryBindgenEvent {
    /// Get the id of the event
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Create a new IPC event.
    pub(crate) fn ipc(id: u64, msg: IPCMessage) -> Self {
        Self {
            id,
            event: AppEventVariant::Ipc(msg),
        }
    }

    /// Create a new webview loaded event.
    pub(crate) fn webview_loaded(id: u64) -> Self {
        Self {
            id,
            event: AppEventVariant::WebviewLoaded,
        }
    }

    /// Consume the event and return the inner variant.
    pub(crate) fn into_variant(self) -> AppEventVariant {
        self.event
    }
}

#[derive(Debug, Clone)]
pub(crate) enum AppEventVariant {
    /// An IPC message from JavaScript
    Ipc(IPCMessage),
    /// The webview has finished loading
    WebviewLoaded,
}

/// Map of evaluate_id → oneshot sender for Respond routing.
/// Each `flush_and_then` call inserts a sender keyed by evaluate_id.
/// When a Respond arrives, start_send reads the evaluate_id and routes it.
pub(crate) type RespondMap =
    Arc<Mutex<alloc::collections::BTreeMap<u32, futures_channel::oneshot::Sender<IPCMessage>>>>;

#[derive(Clone)]
pub(crate) struct IPCSenders {
    eval_sender: Sender<IPCMessage>,
    respond_map: RespondMap,
    app_thread: Arc<OnceLock<Thread>>,
}

impl IPCSenders {
    pub(crate) fn start_send(&self, msg: IPCMessage) {
        match msg.ty().unwrap() {
            crate::ipc::MessageType::Evaluate => {
                self.eval_sender
                    .try_send(msg)
                    .expect("Failed to send evaluate message");
            }
            crate::ipc::MessageType::Respond => {
                let eval_id = msg
                    .respond_evaluate_id()
                    .expect("Failed to read evaluate_id from Respond");
                let tx = self
                    .respond_map
                    .lock()
                    .remove(&eval_id)
                    .unwrap_or_else(|| {
                        panic!("Respond with evaluate_id={eval_id} but no one is waiting for it")
                    });
                tx.send(msg).ok();
            }
        }
        // Wake the app thread if it's parked in wait_for_respond
        if let Some(thread) = self.app_thread.get() {
            thread.unpark();
        }
    }
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub(crate) struct WryIPC {
    pub(crate) proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    eval_receiver: Receiver<IPCMessage>,
    respond_map: RespondMap,
    app_thread: Arc<OnceLock<Thread>>,
}

impl WryIPC {
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>) -> (Self, IPCSenders) {
        let (eval_sender, eval_receiver) = async_channel::unbounded();
        let respond_map: RespondMap =
            Arc::new(Mutex::new(alloc::collections::BTreeMap::new()));
        let app_thread: Arc<OnceLock<Thread>> = Arc::new(OnceLock::new());
        let senders = IPCSenders {
            eval_sender,
            respond_map: respond_map.clone(),
            app_thread: app_thread.clone(),
        };
        let ipc = Self {
            proxy,
            eval_receiver,
            respond_map,
            app_thread,
        };
        (ipc, senders)
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, id: u64, responder: IPCMessage) {
        (self.proxy)(WryBindgenEvent::ipc(id, responder));
    }
}

/// Register a oneshot channel in the respond_map for the given evaluate_id.
///
/// MUST be called BEFORE sending the Evaluate via proxy, to avoid a race
/// where the Respond arrives before the map entry exists.
pub(crate) fn register_respond_receiver(
    eval_id: u32,
) -> futures_channel::oneshot::Receiver<IPCMessage> {
    let (tx, rx) = futures_channel::oneshot::channel::<IPCMessage>();
    with_runtime(|runtime| {
        runtime.ipc().respond_map.lock().insert(eval_id, tx);
    });
    rx
}

/// Wait for the Respond to our most recent Evaluate, processing any
/// interleaved callback Evaluates along the way.
///
/// Uses a per-call one-shot channel so nested calls each get their own Respond.
/// Uses only `try_recv` on the eval channel (no async waker registration) to avoid
/// waker conflicts with `handle_callbacks`. The sender calls `thread::unpark` to
/// wake us from `park_timeout`.
///
/// The caller MUST have already called `register_respond_receiver` and inserted
/// into the respond_map BEFORE sending the Evaluate via proxy.
pub(crate) fn wait_for_respond<O>(
    mut rx: futures_channel::oneshot::Receiver<IPCMessage>,
    with_respond: impl for<'a> Fn(DecodedData<'a>) -> O,
) -> O {
    // Register the app thread so the sender can unpark us
    with_runtime(|runtime| {
        runtime
            .ipc()
            .app_thread
            .get_or_init(|| std::thread::current());
    });

    let eval_receiver = with_runtime(|runtime| runtime.ipc().eval_receiver.clone());

    loop {
        // 1. Check if Respond has arrived (non-blocking)
        match rx.try_recv() {
            Ok(Some(msg)) => {
                let mut data = match msg.decoded().expect("Failed to decode response") {
                    DecodedVariant::Respond { data } => data,
                    _ => unreachable!("Expected Respond from oneshot"),
                };
                // Skip the evaluate_id field (already used for routing)
                let _ = data.take_u32();
                return with_respond(data);
            }
            Ok(None) => {} // Not ready yet
            Err(_) => panic!("Respond oneshot cancelled"),
        }

        // 2. Drain all available Evaluate messages (non-blocking, no waker registration)
        let mut processed_any = false;
        while let Ok(msg) = eval_receiver.try_recv() {
            let decoder = msg.decoded().expect("Failed to decode eval message");
            match decoder {
                DecodedVariant::Evaluate { mut data } => {
                    handle_rust_callback(&mut data);
                    processed_any = true;
                }
                DecodedVariant::Respond { .. } => {
                    unreachable!("Respond should go through oneshot, not eval channel")
                }
            }
        }

        // 3. If we processed callbacks, loop immediately to re-check Respond
        if processed_any {
            continue;
        }

        // 4. Nothing available - park briefly. The sender will unpark us
        //    immediately when a message arrives, so this timeout is just a safety net.
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

pub async fn handle_callbacks() {
    // Use a short sleep between polls to avoid busy-waiting while still
    // allowing wait_for_respond to have exclusive access to eval_receiver's wakers
    // during synchronous blocking.
    let receiver = with_runtime(|runtime| runtime.ipc().eval_receiver.clone());
    loop {
        // Try non-blocking first
        match receiver.try_recv() {
            Ok(response) => {
                let decoder = response.decoded().expect("Failed to decode response");
                match decoder {
                    DecodedVariant::Respond { .. } => unreachable!(),
                    DecodedVariant::Evaluate { mut data } => {
                        handle_rust_callback(&mut data);
                    }
                }
                continue; // drain all available messages before sleeping
            }
            Err(async_channel::TryRecvError::Empty) => {
                // No messages available. Yield and wait for a notification.
                // We use recv() here, which registers a waker. This is safe
                // because wait_for_respond is NOT running during async execution
                // (it blocks the thread, preventing this task from running).
                let Ok(response) = receiver.recv().await else {
                    break;
                };
                let decoder = response.decoded().expect("Failed to decode response");
                match decoder {
                    DecodedVariant::Respond { .. } => unreachable!(),
                    DecodedVariant::Evaluate { mut data } => {
                        handle_rust_callback(&mut data);
                    }
                }
            }
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }
}

/// Handle a Rust callback invocation from JavaScript.
fn handle_rust_callback(data: &mut DecodedData) {
    let fn_id = data.take_u32().expect("Failed to read fn_id");
    let response = match fn_id {
        // Call a registered Rust callback
        0 => {
            let key = data.take_u32().unwrap();

            // Clone the Rc while briefly borrowing the batch state, then release the borrow.
            // This allows nested callbacks to access the object store during our callback execution.
            let callback = with_runtime(|state| {
                let rust_callback = state.get_object::<RustCallback>(key);

                rust_callback.clone_rc()
            });

            // Push a borrow frame before calling the callback - nested calls won't clear our borrowed refs
            with_runtime(|state| state.push_borrow_frame());

            // Call through the cloned Rc (uniform Fn interface)
            let response = IPCMessage::new_respond(|encoder| {
                (callback)(data, encoder);
            });

            // Pop the borrow frame after the callback completes
            with_runtime(|state| state.pop_borrow_frame());

            response
        }
        // Drop a native Rust object when JS GC'd the wrapper
        DROP_NATIVE_REF_FN_ID => {
            let key = ObjectHandle::decode(data).expect("Failed to decode object handle");

            // Remove the object from the thread-local encoder
            remove_object::<RustCallback>(key);

            // Send empty response
            IPCMessage::new_respond(|_| {})
        }
        // Call an exported Rust struct method
        CALL_EXPORT_FN_ID => {
            // Read the export name
            let export_name: alloc::string::String =
                crate::encode::BinaryDecode::decode(data).expect("Failed to decode export name");

            // Find the export handler
            let export = crate::inventory::iter::<crate::JsExportSpec>()
                .find(|e| e.name == export_name)
                .unwrap_or_else(|| panic!("Unknown export: {export_name}"));

            // Call the handler
            let result = (export.handler)(data);

            assert!(data.is_empty(), "Extra data remaining after export call");

            // Send response
            match result {
                Ok(encoded) => IPCMessage::new_respond(|encoder| {
                    encoder.extend(&encoded);
                }),
                Err(err) => {
                    panic!("Export call failed: {err}");
                }
            }
        }
        _ => todo!(),
    };
    with_runtime(|runtime| runtime.ipc().js_response(runtime.webview_id(), response));
}
