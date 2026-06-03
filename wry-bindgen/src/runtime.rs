//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use core::task::{Context, Poll, Waker};
use std::sync::{Arc, Condvar, Mutex};

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::MessageType;
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage, OutboundIPCMessage};
use crate::object_store::ObjectHandle;

#[derive(Debug, Clone)]
pub(crate) enum LockAcquired {
    /// JS entered the handler only to hand Rust the JS-call capability.
    Empty,
    /// JS entered the handler with an inbound Evaluate that Rust must dispatch.
    InboundEvaluate(IPCMessage),
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
    /// Rust wants JS to enter the synchronous handler and park an XHR.
    AcquireLock,
    /// Rust is returning to the JS event loop with no JS payload.
    ReleaseLock,
}

pub(crate) struct IPCSenders {
    slots: Arc<IPCSingleSlots>,
}

impl IPCSenders {
    fn new(slots: Arc<IPCSingleSlots>) -> Self {
        slots.add_sender();
        Self { slots }
    }

    pub(crate) fn start_send(&self, msg: IPCMessage) -> bool {
        self.slots.send_ipc(msg)
    }

    pub(crate) fn lock_acquired(&self, acquired: LockAcquired) -> bool {
        self.slots.send_lock(acquired)
    }
}

impl Clone for IPCSenders {
    fn clone(&self) -> Self {
        self.slots.add_sender();
        Self {
            slots: self.slots.clone(),
        }
    }
}

impl Drop for IPCSenders {
    fn drop(&mut self) {
        self.slots.drop_sender();
    }
}

struct IPCSingleSlots {
    state: Mutex<IPCSingleSlotState>,
    blocking_recv: Condvar,
}

#[derive(Default)]
struct IPCSingleSlotState {
    ipc: Option<IPCMessage>,
    lock: Option<LockAcquired>,
    lock_waker: Option<core::task::Waker>,
    sender_count: usize,
    closed: bool,
}

impl IPCSingleSlots {
    fn new() -> Self {
        Self {
            state: Mutex::new(IPCSingleSlotState::default()),
            blocking_recv: Condvar::new(),
        }
    }

    fn add_sender(&self) {
        let mut state = self.state.lock().unwrap();
        state.sender_count += 1;
    }

    fn drop_sender(&self) {
        let (should_notify, waker) = {
            let mut state = self.state.lock().unwrap();
            let Some(sender_count) = state.sender_count.checked_sub(1) else {
                return;
            };
            state.sender_count = sender_count;
            if sender_count == 0 {
                (true, Self::close_state(&mut state))
            } else {
                (false, None)
            }
        };
        if should_notify {
            self.notify_closed(waker);
        }
    }

    fn send_ipc(&self, msg: IPCMessage) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.closed || state.ipc.is_some() {
            return false;
        }
        state.ipc = Some(msg);
        drop(state);
        self.blocking_recv.notify_one();
        true
    }

    fn send_lock(&self, acquired: LockAcquired) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return false;
        }
        if state.lock.is_some() {
            return false;
        }
        state.lock = Some(acquired);
        let waker = state.lock_waker.take();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
        true
    }

    fn poll_lock(&self, cx: &mut Context<'_>) -> Poll<Option<LockAcquired>> {
        let mut state = self.state.lock().unwrap();
        if let Some(value) = state.lock.take() {
            Poll::Ready(Some(value))
        } else if state.closed {
            Poll::Ready(None)
        } else {
            state.lock_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    fn recv_blocking(&self) -> Option<IPCMessage> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(msg) = state.ipc.take() {
                return Some(msg);
            }
            if state.closed {
                return None;
            }
            state = self.blocking_recv.wait(state).unwrap();
        }
    }

    fn close(&self) {
        let waker = {
            let mut state = self.state.lock().unwrap();
            Self::close_state(&mut state)
        };
        self.notify_closed(waker);
    }

    fn close_state(state: &mut IPCSingleSlotState) -> Option<Waker> {
        if state.closed {
            None
        } else {
            state.closed = true;
            state.lock_waker.take()
        }
    }

    fn notify_closed(&self, waker: Option<Waker>) {
        self.blocking_recv.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
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

    pub(crate) fn request_js_lock(&self, id: u64) {
        (self.proxy)(WryBindgenEvent::acquire_lock(id));
    }

    pub(crate) fn poll_lock_acquired(&self, cx: &mut Context<'_>) -> Poll<Option<LockAcquired>> {
        self.slots.poll_lock(cx)
    }
}

impl Drop for WryIPC {
    fn drop(&mut self) {
        self.slots.close();
    }
}

pub(crate) fn progress_js_with<O>(
    mut with_respond: impl for<'a> FnMut(DecodedData<'a>) -> O,
) -> Option<O> {
    let slots = with_runtime(|runtime| runtime.ipc().slots.clone());
    let response = slots.recv_blocking()?;
    dispatch_inbound_message(&response, &mut with_respond)
}

pub(crate) fn handle_inbound_ipc(response: &IPCMessage) {
    dispatch_inbound_message(response, &mut |_| unreachable!());
}

fn dispatch_inbound_message<O>(
    response: &IPCMessage,
    with_respond: &mut impl for<'a> FnMut(DecodedData<'a>) -> O,
) -> Option<O> {
    let decoder = response.decoded().expect("Failed to decode response");
    match decoder {
        DecodedVariant::Respond { data } => {
            with_runtime(|runtime| {
                // JS has now consumed the Rust→JS Evaluate this Respond
                // closes, so types it carried can be sent as `TYPE_CACHED`
                // from here on.
                runtime.pop_and_ack_type_cache_frame();
            });
            let result = with_respond(data);
            Some(result)
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
    let mut encoder = crate::ipc::EncodedData::new();
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

    fn ipc_message(message_type: MessageType) -> IPCMessage {
        let mut data = EncodedData::new();
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

        assert!(slots.send_ipc(ipc_message(MessageType::Evaluate)));
        assert!(!slots.send_ipc(ipc_message(MessageType::Respond)));

        let received = slots.recv_blocking().expect("first message should remain");
        assert_eq!(received.ty().unwrap(), MessageType::Evaluate);

        assert!(slots.send_ipc(ipc_message(MessageType::Respond)));
        let received = slots
            .recv_blocking()
            .expect("slot should accept after take");
        assert_eq!(received.ty().unwrap(), MessageType::Respond);
    }

    #[test]
    fn closed_single_slots_reject_new_messages() {
        let slots = IPCSingleSlots::new();

        slots.close();

        assert!(!slots.send_ipc(ipc_message(MessageType::Evaluate)));
        assert!(!slots.send_lock(LockAcquired::Empty));
        assert!(slots.recv_blocking().is_none());
    }

    #[test]
    fn dropping_last_ipc_sender_closes_slots() {
        let (ipc, senders) = WryIPC::new(Arc::new(|_| {}));
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_lock_acquired(&mut cx), Poll::Pending));

        drop(senders);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_lock_acquired(&mut cx), Poll::Ready(None)));
    }

    #[test]
    fn ipc_sender_clone_lifetime() {
        let (ipc, sender) = WryIPC::new(Arc::new(|_| {}));
        let sender_clone = sender.clone();
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_lock_acquired(&mut cx), Poll::Pending));

        drop(sender);

        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        assert!(matches!(ipc.poll_lock_acquired(&mut cx), Poll::Pending));

        drop(sender_clone);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_lock_acquired(&mut cx), Poll::Ready(None)));
    }
}
