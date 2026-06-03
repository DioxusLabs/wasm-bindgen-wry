//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use core::task::{Context, Poll};
use std::sync::{Arc, Condvar, Mutex};

use atomic_waker::AtomicWaker;

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::MessageType;
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage, OutboundIPCMessage};
use crate::object_store::ObjectHandle;

/// An inbound item arriving from JS on the single shared channel.
///
/// Whichever waiter is currently active consumes it: the synchronous
/// `recv_blocking` used inside a Rust→JS call, or the async `poll_recv` the
/// driver awaits while idle.
#[derive(Debug, Clone)]
pub(crate) enum Inbound {
    /// A JS→Rust message: a Respond answering an outbound call, or an Evaluate
    /// callback Rust must dispatch.
    Message(IPCMessage),
    /// JS parked a fresh XHR in response to an acquire request; Rust may now
    /// drive JS through it.
    LockReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InboundSendError {
    Closed,
    Occupied,
}

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
    pub(crate) fn ipc(id: u64, msg: OutboundIPCMessage) -> Self {
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

    /// Ask the webview to synchronously enter the wry-bindgen handler and park
    /// an XHR that Rust can use as its JS-call capability.
    pub(crate) fn acquire_lock(id: u64) -> Self {
        Self {
            id,
            event: AppEventVariant::AcquireLock,
        }
    }

    /// Release the currently parked XHR without sending a JS payload.
    pub(crate) fn release_lock(id: u64) -> Self {
        Self {
            id,
            event: AppEventVariant::ReleaseLock,
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
    Ipc(OutboundIPCMessage),
    /// The webview has finished loading
    WebviewLoaded,
    /// Ask JS to enter the synchronous handler and park an XHR.
    AcquireLock,
    /// Release the currently parked XHR with a blank reply.
    ReleaseLock,
}

#[derive(Clone)]
pub(crate) struct IPCSenders(Arc<IPCSenderSet>);

struct IPCSenderSet {
    slots: Arc<IPCSingleSlots>,
}

impl IPCSenders {
    fn new(slots: Arc<IPCSingleSlots>) -> Self {
        Self(Arc::new(IPCSenderSet { slots }))
    }

    pub(crate) fn send(&self, inbound: Inbound) -> Result<(), InboundSendError> {
        self.0.slots.send(inbound)
    }
}

// Closing on the last shared sender drop preserves channel-like shutdown
// semantics without keeping a sender count in the receive state.
impl Drop for IPCSenderSet {
    fn drop(&mut self) {
        self.slots.close();
    }
}

struct IPCSingleSlots {
    state: Mutex<IPCSingleSlotState>,
    blocking_recv: Condvar,
    recv_waker: AtomicWaker,
}

#[derive(Default)]
struct IPCSingleSlotState {
    /// The single pending inbound item. JS is synchronously blocked whenever an
    /// XHR is parked, so at most one item is ever outstanding.
    slot: Option<Inbound>,
    closed: bool,
}

impl IPCSingleSlots {
    fn new() -> Self {
        Self {
            state: Mutex::new(IPCSingleSlotState::default()),
            blocking_recv: Condvar::new(),
            recv_waker: AtomicWaker::new(),
        }
    }

    /// Deliver an inbound item. The slot is single-capacity (JS is blocked while
    /// an XHR is parked, so nothing else can arrive). Both waiters are signalled;
    /// whichever is active consumes it and the other notification is a no-op.
    fn send(&self, inbound: Inbound) -> Result<(), InboundSendError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(InboundSendError::Closed);
        }
        if state.slot.is_some() {
            return Err(InboundSendError::Occupied);
        }
        state.slot = Some(inbound);
        drop(state);
        self.blocking_recv.notify_one();
        self.recv_waker.wake();
        Ok(())
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Inbound>> {
        let mut state = self.state.lock().unwrap();
        if let Some(value) = state.slot.take() {
            Poll::Ready(Some(value))
        } else if state.closed {
            Poll::Ready(None)
        } else {
            // Readiness and registration are both protected by `state`, so a
            // sender cannot fill the slot between the empty check and register.
            self.recv_waker.register(cx.waker());
            Poll::Pending
        }
    }

    fn recv_blocking(&self) -> Option<IPCMessage> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(inbound) = state.slot.take() {
                match inbound {
                    Inbound::Message(msg) => return Some(msg),
                    // Empty locks are only requested by the idle driver, which
                    // waits via `poll_recv`; they never reach a blocking JS call.
                    Inbound::LockReady => {
                        unreachable!("LockReady delivered to a blocking JS-call waiter")
                    }
                }
            }
            if state.closed {
                return None;
            }
            state = self.blocking_recv.wait(state).unwrap();
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.closed = true;
        drop(state);
        self.blocking_recv.notify_all();
        self.recv_waker.wake();
    }
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages the single pending IPC slot.
pub(crate) struct WryIPC {
    pub(crate) proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    slots: Arc<IPCSingleSlots>,
}

impl WryIPC {
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>) -> (Self, IPCSenders) {
        let slots = Arc::new(IPCSingleSlots::new());
        let senders = IPCSenders::new(slots.clone());
        let ipc = Self { proxy, slots };
        (ipc, senders)
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, id: u64, responder: OutboundIPCMessage) {
        (self.proxy)(WryBindgenEvent::ipc(id, responder));
    }

    /// Ask the main thread to have JS park an XHR (the acquire half of the lock).
    pub(crate) fn send_acquire_lock(&self, id: u64) {
        (self.proxy)(WryBindgenEvent::acquire_lock(id));
    }

    pub(crate) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Inbound>> {
        self.slots.poll_recv(cx)
    }
}

impl Drop for WryIPC {
    fn drop(&mut self) {
        self.slots.close();
    }
}

pub(crate) fn progress_js_with<O>(
    with_respond: impl for<'a> FnMut(DecodedData<'a>) -> O,
) -> Option<O> {
    let slots = with_runtime(|runtime| runtime.ipc().slots.clone());
    let response = slots.recv_blocking()?;
    dispatch_inbound_message(&response).map(with_respond)
}

pub(crate) fn dispatch_inbound_message(response: &IPCMessage) -> Option<DecodedData<'_>> {
    let decoder = response.decoded().expect("Failed to decode response");
    match decoder {
        DecodedVariant::Respond { data } => {
            with_runtime(|runtime| {
                // JS has now consumed the Rust→JS Evaluate this Respond
                // closes, so types it carried can be sent as `TYPE_CACHED`
                // from here on.
                runtime.pop_and_ack_type_cache_frame();
            });
            Some(data)
        }
        DecodedVariant::Evaluate { data } => {
            handle_inbound_evaluate(data);
            None
        }
    }
}

fn handle_inbound_evaluate(mut data: DecodedData<'_>) {
    handle_rust_callback(&mut data);
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

            // Push a borrow frame before calling the callback - nested calls
            // won't clear our borrowed refs. The guard pops the frame even if
            // the callback panics.
            let _frame = BorrowFrameGuard::new();

            let mut encoder = respond_encoder();
            // Call through the cloned Rc (uniform Fn interface). A decode error
            // surfaces here with context instead of an opaque `unwrap` panic
            // inside the callback trampoline (mirrors the export path below).
            match (callback)(data, &mut encoder) {
                Ok(()) => finish_respond_message(encoder),
                Err(err) => {
                    panic!("Rust callback {key} failed to decode arguments: {err}")
                }
            }
        }
        // Drop a native Rust object when JS GC'd the wrapper
        DROP_NATIVE_REF_FN_ID => {
            let key = ObjectHandle::decode(data).expect("Failed to decode object handle");

            // The Rust owner may have dropped this closure before JS GC runs.
            crate::object_store::drop_object(key);

            finish_respond_message(respond_encoder())
        }
        // Call an exported Rust struct method
        CALL_EXPORT_FN_ID => {
            // Read the export name
            let export_name: alloc::string::String =
                crate::encode::BinaryDecode::decode(data).expect("Failed to decode export name");

            // Find the export handler
            let export = crate::__rt::inventory::iter::<crate::__rt::JsExportSpec>()
                .find(|e| e.name == export_name)
                .unwrap_or_else(|| panic!("Unknown export: {export_name}"));

            // Call the handler
            let result = (export.handler)(data);

            assert!(data.is_empty(), "Extra data remaining after export call");

            // Send response
            match result {
                Ok(encoded) => {
                    let mut encoder = respond_encoder();
                    encoder.extend(&encoded);
                    finish_respond_message(encoder)
                }
                Err(err) => {
                    panic!("Export call failed: {err}");
                }
            }
        }
        _ => panic!("Unknown Rust callback function ID: {fn_id}"),
    };
    with_runtime(|runtime| runtime.ipc().js_response(runtime.webview_id(), response));
}

/// Scopes a borrow frame for the duration of a callback. The frame is pushed on
/// construction and popped on drop, so it survives a panicking callback.
struct BorrowFrameGuard;

impl BorrowFrameGuard {
    fn new() -> Self {
        with_runtime(|state| state.push_borrow_frame());
        Self
    }
}

impl Drop for BorrowFrameGuard {
    fn drop(&mut self) {
        with_runtime(|state| state.pop_borrow_frame());
    }
}

fn respond_encoder() -> crate::ipc::EncodedData {
    let mut encoder = crate::ipc::EncodedData::default();
    encoder.push_u8(MessageType::Respond as u8);
    encoder
}

fn finish_respond_message(encoder: crate::ipc::EncodedData) -> OutboundIPCMessage {
    with_runtime(|runtime| runtime.finish_respond_message(encoder))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::EncodedData;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    fn ipc_message(message_type: MessageType) -> IPCMessage {
        let mut data = EncodedData::default();
        data.push_u8(message_type as u8);
        IPCMessage::new(data.to_bytes())
    }

    struct CountWaker {
        wakes: Arc<AtomicUsize>,
    }

    impl std::task::Wake for CountWaker {
        fn wake(self: Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counting_waker() -> (Waker, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let waker = Waker::from(Arc::new(CountWaker {
            wakes: wakes.clone(),
        }));
        (waker, wakes)
    }

    #[test]
    fn ipc_single_slot_rejects_second_pending_message() {
        let slots = IPCSingleSlots::new();

        assert_eq!(
            slots.send(Inbound::Message(ipc_message(MessageType::Evaluate))),
            Ok(())
        );
        assert_eq!(
            slots.send(Inbound::Message(ipc_message(MessageType::Respond))),
            Err(InboundSendError::Occupied)
        );

        let received = slots.recv_blocking().expect("first message should remain");
        assert!(matches!(
            received.decoded().unwrap(),
            DecodedVariant::Evaluate { .. }
        ));

        assert_eq!(
            slots.send(Inbound::Message(ipc_message(MessageType::Respond))),
            Ok(())
        );
        let received = slots
            .recv_blocking()
            .expect("slot should accept after take");
        assert!(matches!(
            received.decoded().unwrap(),
            DecodedVariant::Respond { .. }
        ));
    }

    #[test]
    fn closed_single_slots_reject_new_messages() {
        let slots = IPCSingleSlots::new();

        slots.close();

        assert_eq!(
            slots.send(Inbound::Message(ipc_message(MessageType::Evaluate))),
            Err(InboundSendError::Closed)
        );
        assert_eq!(
            slots.send(Inbound::LockReady),
            Err(InboundSendError::Closed)
        );
        assert!(slots.recv_blocking().is_none());
    }

    #[test]
    fn dropping_last_ipc_sender_closes_slots() {
        let (ipc, senders) = WryIPC::new(Arc::new(|_| {}));
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Pending));

        drop(senders);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Ready(None)));
    }

    #[test]
    fn ipc_sender_clone_lifetime() {
        let (ipc, sender) = WryIPC::new(Arc::new(|_| {}));
        let sender_clone = sender.clone();
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Pending));

        drop(sender);

        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Pending));

        drop(sender_clone);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Ready(None)));
    }
}
