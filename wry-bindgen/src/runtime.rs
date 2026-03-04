//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

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
    /// Waker for the async `handle_callbacks` task.
    /// `handle_callbacks` uses `try_recv` (no waker on the channel) so that
    /// `recv_blocking` in `wait_for_respond` is the sole channel listener.
    async_waker: Arc<Mutex<Option<Waker>>>,
}

impl IPCSenders {
    pub(crate) fn start_send(&self, msg: IPCMessage) {
        self.sender
            .try_send(msg)
            .expect("Failed to send message");
        if let Ok(guard) = self.async_waker.lock() {
            if let Some(waker) = guard.as_ref() {
                waker.wake_by_ref();
            }
        }
    }
}

/// The runtime environment for communicating with JavaScript.
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

/// Wait for the Respond matching `eval_id`, processing interleaved callbacks.
///
/// All messages (Evaluates and Responds) share one channel. `recv_blocking()`
/// blocks efficiently; it is the sole channel listener because `handle_callbacks`
/// uses `try_recv` + a separate waker.
pub(crate) fn wait_for_respond<O>(
    eval_id: u32,
    with_respond: impl for<'a> Fn(DecodedData<'a>) -> O,
) -> O {
    let receiver = with_runtime(|runtime| runtime.ipc().receiver.clone());

    #[cfg(debug_assertions)]
    let start = std::time::Instant::now();

    loop {
        // 1. Check stash for our Respond (a nested call may have stashed it)
        let stashed = with_runtime(|rt| {
            if let Some(idx) = rt.stashed_responds.iter().position(|m| {
                m.respond_evaluate_id()
                    .map(|id| id == eval_id)
                    .unwrap_or(false)
            }) {
                Some(rt.stashed_responds.swap_remove(idx))
            } else {
                None
            }
        });
        if let Some(msg) = stashed {
            let data = match msg.decoded().expect("Failed to decode stashed Respond") {
                DecodedVariant::Respond { mut data } => {
                    let _ = data.take_u32(); // skip evaluate_id
                    data
                }
                _ => unreachable!(),
            };
            return with_respond(data);
        }

        // 2. Drain available messages without blocking
        loop {
            match receiver.try_recv() {
                Ok(msg) => {
                    if dispatch_message(msg, eval_id) {
                        break; // our Respond was stashed — go pick it up
                    }
                }
                Err(_) => break,
            }
        }

        // Re-check stash (dispatch_message may have stashed our Respond)
        let found = with_runtime(|rt| {
            rt.stashed_responds
                .iter()
                .any(|m| m.respond_evaluate_id().map(|id| id == eval_id).unwrap_or(false))
        });
        if found {
            continue;
        }

        // 3. Block until a message arrives
        let msg = receiver
            .recv_blocking()
            .expect("channel closed unexpectedly");
        dispatch_message(msg, eval_id);

        #[cfg(debug_assertions)]
        if start.elapsed() > Duration::from_secs(30) {
            panic!("wait_for_respond timed out after 30s waiting for evaluate_id={eval_id}");
        }
    }
}

/// Process one message: Evaluates are handled inline, Responds are stashed.
/// Returns true if the message was a Respond for `our_eval_id`.
fn dispatch_message(msg: IPCMessage, our_eval_id: u32) -> bool {
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
            let is_ours = msg
                .respond_evaluate_id()
                .map(|id| id == our_eval_id)
                .unwrap_or(false);
            with_runtime(|rt| rt.stashed_responds.push(msg));
            is_ours
        }
    }
}

pub async fn handle_callbacks() {
    let receiver = with_runtime(|runtime| runtime.ipc().receiver.clone());
    let waker_slot = with_runtime(|runtime| runtime.ipc().async_waker.clone());
    loop {
        // Drain all available messages (no waker on channel)
        while let Ok(msg) = receiver.try_recv() {
            handle_ipc_message(msg);
        }
        // Register waker and wait for start_send to wake us
        let closed = std::future::poll_fn(|cx| {
            if let Ok(mut guard) = waker_slot.lock() {
                *guard = Some(cx.waker().clone());
            }
            // Re-check after registering (avoid missed wake)
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

fn handle_ipc_message(msg: IPCMessage) {
    let ty = msg.ty().expect("Failed to read message type");
    match ty {
        MessageType::Respond => {
            with_runtime(|rt| rt.stashed_responds.push(msg));
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

            let callback = with_runtime(|state| {
                let rust_callback = state.get_object::<RustCallback>(key);
                rust_callback.clone_rc()
            });

            with_runtime(|state| state.push_borrow_frame());

            let response = IPCMessage::new_respond(|encoder| {
                (callback)(data, encoder);
            });

            with_runtime(|state| state.pop_borrow_frame());

            response
        }
        // Drop a native Rust object when JS GC'd the wrapper
        DROP_NATIVE_REF_FN_ID => {
            let key = ObjectHandle::decode(data).expect("Failed to decode object handle");
            remove_object::<RustCallback>(key);
            IPCMessage::new_respond(|_| {})
        }
        // Call an exported Rust struct method
        CALL_EXPORT_FN_ID => {
            let export_name: alloc::string::String =
                crate::encode::BinaryDecode::decode(data).expect("Failed to decode export name");

            let export = crate::inventory::iter::<crate::JsExportSpec>()
                .find(|e| e.name == export_name)
                .unwrap_or_else(|| panic!("Unknown export: {export_name}"));

            let result = (export.handler)(data);

            assert!(data.is_empty(), "Extra data remaining after export call");

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
