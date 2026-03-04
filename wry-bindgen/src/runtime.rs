//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Waker;
use std::time::Duration;

use async_channel::{Receiver, Sender};

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage, MessageType};
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

#[derive(Clone)]
pub(crate) struct IPCSenders {
    sender: Sender<IPCMessage>,
    /// Waker to wake the async `handle_callbacks` task when a message arrives.
    /// `handle_callbacks` uses `try_recv` (no waker on the channel) to avoid
    /// conflicting with `recv_blocking` in `wait_for_respond`.
    async_waker: Arc<Mutex<Option<Waker>>>,
}

impl IPCSenders {
    pub(crate) fn start_send(&self, msg: IPCMessage) {
        self.sender
            .try_send(msg)
            .expect("Failed to send message");
        // Wake either the async handle_callbacks task or the blocking
        // wait_for_respond call — whichever is currently waiting.
        if let Ok(guard) = self.async_waker.lock() {
            if let Some(waker) = guard.as_ref() {
                waker.wake_by_ref();
            }
        }
    }
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub(crate) struct WryIPC {
    pub(crate) proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    receiver: Receiver<IPCMessage>,
    async_waker: Arc<Mutex<Option<Waker>>>,
}

impl WryIPC {
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(
        proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    ) -> (Self, IPCSenders) {
        let (sender, receiver) = async_channel::unbounded();
        let async_waker: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let senders = IPCSenders {
            sender,
            async_waker: async_waker.clone(),
        };
        let ipc = Self {
            proxy,
            receiver,
            async_waker,
        };
        (ipc, senders)
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, id: u64, responder: IPCMessage) {
        (self.proxy)(WryBindgenEvent::ipc(id, responder));
    }
}

// Thread-local stash for Responds that arrived out of order.
// When a nested `wait_for_respond` receives a Respond for an outer call,
// it stashes it here. The outer call checks the stash before blocking.
thread_local! {
    static STASHED_RESPONDS: RefCell<alloc::vec::Vec<IPCMessage>> = const { RefCell::new(alloc::vec::Vec::new()) };
}

/// Wait for the Respond to our Evaluate (identified by `eval_id`), processing
/// any interleaved callback Evaluates along the way.
///
/// Both Evaluates (callbacks from JS) and Responds (results of our flushes)
/// arrive on the same channel. We use `recv_blocking()` to block efficiently
/// without polling or parking. This is safe because `handle_callbacks` never
/// calls `recv().await` on the channel — it uses `try_recv` + a separate waker.
pub(crate) fn wait_for_respond<O>(
    eval_id: u32,
    with_respond: impl for<'a> Fn(DecodedData<'a>) -> O,
) -> O {
    let receiver = with_runtime(|runtime| runtime.ipc().receiver.clone());

    #[cfg(debug_assertions)]
    let start = std::time::Instant::now();

    loop {
        // 1. Check the stash for our Respond (may have been stashed by a nested call)
        let stashed = STASHED_RESPONDS.with(|s| {
            let mut stash = s.borrow_mut();
            if let Some(idx) = stash.iter().position(|m| {
                m.respond_evaluate_id()
                    .map(|id| id == eval_id)
                    .unwrap_or(false)
            }) {
                Some(stash.swap_remove(idx))
            } else {
                None
            }
        });
        if let Some(msg) = stashed {
            let data = match msg.decoded().expect("Failed to decode stashed Respond") {
                DecodedVariant::Respond { mut data } => {
                    let _ = data.take_u32(); // skip evaluate_id (already matched)
                    data
                }
                _ => unreachable!("Stashed message was not a Respond"),
            };
            return with_respond(data);
        }

        // 2. Drain all available messages without blocking first
        let mut found_ours = false;
        while let Ok(msg) = receiver.try_recv() {
            if process_or_stash(msg, eval_id) {
                found_ours = true;
                break;
            }
        }
        if found_ours {
            // Our Respond was stashed by process_or_stash — loop to pick it up from stash
            continue;
        }

        // 3. Block until a message arrives on the channel.
        //    Safe because handle_callbacks only uses try_recv (no waker on channel).
        let msg = receiver
            .recv_blocking()
            .expect("channel closed unexpectedly");

        if process_or_stash(msg, eval_id) {
            // Our Respond was stashed — loop to pick it up
            continue;
        }

        #[cfg(debug_assertions)]
        if start.elapsed() > Duration::from_secs(30) {
            panic!("wait_for_respond timed out after 30s waiting for evaluate_id={eval_id}");
        }
    }
}

/// Process a message from the channel. Returns true if it was a Respond for `our_eval_id`
/// (stashed for pickup). Evaluates are processed inline.
fn process_or_stash(msg: IPCMessage, our_eval_id: u32) -> bool {
    let ty = msg.ty().expect("Failed to read message type");
    match ty {
        MessageType::Evaluate => {
            let mut data = match msg.decoded().expect("Failed to decode Evaluate") {
                DecodedVariant::Evaluate { data } => data,
                _ => unreachable!(),
            };
            handle_rust_callback(&mut data);
            false
        }
        MessageType::Respond => {
            // Stash it (including our own — the main loop will pick it up)
            STASHED_RESPONDS.with(|s| s.borrow_mut().push(msg));
            // Return true if this was for us
            let last = STASHED_RESPONDS.with(|s| {
                let stash = s.borrow();
                stash
                    .last()
                    .unwrap()
                    .respond_evaluate_id()
                    .map(|id| id == our_eval_id)
                    .unwrap_or(false)
            });
            last
        }
    }
}

pub async fn handle_callbacks() {
    let receiver = with_runtime(|runtime| runtime.ipc().receiver.clone());
    let waker_slot = with_runtime(|runtime| runtime.ipc().async_waker.clone());
    loop {
        // Drain all available messages without registering a waker on the channel
        while let Ok(msg) = receiver.try_recv() {
            handle_ipc_message(msg);
        }
        // No messages available — register our waker and wait.
        // start_send() will call waker.wake_by_ref() when a message arrives.
        let closed = std::future::poll_fn(|cx| {
            // Register the waker so start_send can wake us
            if let Ok(mut guard) = waker_slot.lock() {
                *guard = Some(cx.waker().clone());
            }
            // Check once more after registering (avoid race)
            match receiver.try_recv() {
                Ok(msg) => {
                    handle_ipc_message(msg);
                    core::task::Poll::Ready(false)
                }
                Err(async_channel::TryRecvError::Empty) => core::task::Poll::Pending,
                Err(async_channel::TryRecvError::Closed) => core::task::Poll::Ready(true),
            }
        })
        .await;
        if closed {
            break;
        }
    }
}

/// Handle a single IPC message from the channel.
/// During async execution, only Evaluates should arrive. Responds are stashed
/// as a safety measure (they'll be picked up by the next wait_for_respond).
fn handle_ipc_message(msg: IPCMessage) {
    let ty = msg.ty().expect("Failed to read message type");
    match ty {
        MessageType::Respond => {
            // Shouldn't happen during async execution, but stash it safely
            STASHED_RESPONDS.with(|s| s.borrow_mut().push(msg));
        }
        MessageType::Evaluate => {
            let mut data = match msg.decoded().expect("Failed to decode Evaluate") {
                DecodedVariant::Evaluate { data } => data,
                _ => unreachable!(),
            };
            handle_rust_callback(&mut data);
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
