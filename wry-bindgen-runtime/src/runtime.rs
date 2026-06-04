//! Runtime IPC for one wry-bindgen session.
//!
//! This module owns the channel pair between the app-thread runtime and the
//! main-thread driver. It has no dependency on a concrete window event loop.

<<<<<<< /tmp/ours_runtime.rs
use core::task::{Context, Poll, Waker};
use std::sync::{Arc, Condvar, Mutex};
||||||| /tmp/base_runtime.rs
use core::pin::Pin;
use std::sync::Arc;

use alloc::boxed::Box;
use async_channel::{Receiver, Sender};
use futures_util::{FutureExt, StreamExt};
use spin::RwLock;
=======
use core::task::{Context, Poll};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, Weak};

use atomic_waker::AtomicWaker;
>>>>>>> /tmp/theirs_runtime.rs

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};
use crate::object_store::ObjectHandle;
use wry_bindgen_abi::unstable::JsExportSpecRuntime;

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

#[derive(Debug, Clone)]
pub(crate) enum DriverCommand {
    /// Ask the webview to synchronously enter the handler and park an XHR.
    AcquireLock,
    /// Deliver a Rust-to-JS IPC payload through the parked XHR.
    SendIpc(OutboundIPCMessage),
    /// Release the currently parked XHR with a blank response.
    ReleaseLock,
}

#[derive(Clone)]
pub(crate) struct DriverCommandSender(Arc<DriverCommandSenderSet>);

#[derive(Clone)]
pub(crate) struct DriverCommandWeakSender(Weak<DriverCommandQueue>);

pub(crate) struct DriverCommandReceiver {
    queue: Arc<DriverCommandQueue>,
}

struct DriverCommandSenderSet {
    queue: Arc<DriverCommandQueue>,
}

impl DriverCommandSender {
    fn new(queue: Arc<DriverCommandQueue>) -> Self {
        Self(Arc::new(DriverCommandSenderSet { queue }))
    }

<<<<<<< /tmp/ours_runtime.rs
    /// Create a new IPC event.
    pub(crate) fn ipc(id: u64, msg: IPCMessage) -> Self {
        Self {
            id,
            event: AppEventVariant::Ipc(msg),
||||||| /tmp/base_runtime.rs
    /// Create a new IPC event.
    pub(crate) fn ipc(id: u64, msg: OutboundIPCMessage) -> Self {
        Self {
            id,
            event: AppEventVariant::Ipc(msg),
=======
    pub(crate) fn send(&self, command: DriverCommand) {
        self.0.queue.send(command)
    }

    pub(crate) fn downgrade(&self) -> DriverCommandWeakSender {
        DriverCommandWeakSender(Arc::downgrade(&self.0.queue))
    }
}

impl Drop for DriverCommandSenderSet {
    fn drop(&mut self) {
        self.queue.close();
    }
}

impl DriverCommandWeakSender {
    pub(crate) fn send(&self, command: DriverCommand) {
        if let Some(queue) = self.0.upgrade() {
            queue.send(command);
>>>>>>> /tmp/theirs_runtime.rs
        }
    }
}

impl DriverCommandReceiver {
    fn new(queue: Arc<DriverCommandQueue>) -> Self {
        Self { queue }
    }

    pub(crate) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<DriverCommand>> {
        self.queue.poll_recv(cx)
    }
}

struct DriverCommandQueue {
    state: Mutex<DriverCommandQueueState>,
    recv_waker: AtomicWaker,
}

#[derive(Default)]
struct DriverCommandQueueState {
    commands: VecDeque<DriverCommand>,
    closed: bool,
}

impl DriverCommandQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(DriverCommandQueueState::default()),
            recv_waker: AtomicWaker::new(),
        }
    }

<<<<<<< /tmp/ours_runtime.rs
    /// Ask the webview to synchronously enter the wry-bindgen handler and park
    /// an XHR that Rust can use as its JS-call capability.
    pub(crate) fn acquire_lock(id: u64) -> Self {
        Self {
            id,
            event: AppEventVariant::HandlerLock { acquire: true },
        }
    }

    /// Release the currently parked XHR without sending a JS payload.
    pub(crate) fn release_lock(id: u64) -> Self {
        Self {
            id,
            event: AppEventVariant::HandlerLock { acquire: false },
        }
    }

    /// Consume the event and return the inner variant.
    pub(crate) fn into_variant(self) -> AppEventVariant {
        self.event
||||||| /tmp/base_runtime.rs
    /// Consume the event and return the inner variant.
    pub(crate) fn into_variant(self) -> AppEventVariant {
        self.event
=======
    fn send(&self, command: DriverCommand) {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.commands.push_back(command);
        drop(state);
        self.recv_waker.wake();
>>>>>>> /tmp/theirs_runtime.rs
    }

<<<<<<< /tmp/ours_runtime.rs
#[derive(Debug, Clone)]
pub(crate) enum AppEventVariant {
    /// An IPC message from JavaScript
    Ipc(IPCMessage),
    /// The webview has finished loading
    WebviewLoaded,
    /// Control the parked-XHR lock. `acquire` asks JS to enter the synchronous
    /// handler and park an XHR (via `evaluate_script`); `!acquire` releases the
    /// currently parked XHR with a blank reply, handing the event loop back to
    /// JS. Both are main-thread-only actions the app thread can only request
    /// through the event loop.
    HandlerLock { acquire: bool },
||||||| /tmp/base_runtime.rs
#[derive(Debug, Clone)]
pub(crate) enum AppEventVariant {
    /// An IPC message from JavaScript
    Ipc(OutboundIPCMessage),
    /// The webview has finished loading
    WebviewLoaded,
=======
    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<DriverCommand>> {
        let mut state = self.state.lock().unwrap();
        if let Some(command) = state.commands.pop_front() {
            Poll::Ready(Some(command))
        } else if state.closed {
            Poll::Ready(None)
        } else {
            self.recv_waker.register(cx.waker());
            Poll::Pending
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.closed = true;
        drop(state);
        self.recv_waker.wake();
    }
>>>>>>> /tmp/theirs_runtime.rs
}

<<<<<<< /tmp/ours_runtime.rs
pub(crate) struct IPCSenders {
    slots: Arc<IPCSingleSlots>,
||||||| /tmp/base_runtime.rs
#[derive(Clone)]
pub(crate) struct IPCSenders {
    eval_sender: Sender<IPCMessage>,
    respond_sender: futures_channel::mpsc::UnboundedSender<IPCMessage>,
=======
#[derive(Clone)]
pub(crate) struct IPCSenders(Arc<IPCSenderSet>);

struct IPCSenderSet {
    slots: Arc<IPCSingleSlots>,
>>>>>>> /tmp/theirs_runtime.rs
}

impl IPCSenders {
<<<<<<< /tmp/ours_runtime.rs
    fn new(slots: Arc<IPCSingleSlots>) -> Self {
        slots.add_sender();
        Self { slots }
    }

    pub(crate) fn send(&self, inbound: Inbound) -> Result<(), InboundSendError> {
        self.slots.send(inbound)
    }
}

impl Clone for IPCSenders {
    fn clone(&self) -> Self {
        self.slots.add_sender();
        Self {
            slots: self.slots.clone(),
        }
||||||| /tmp/base_runtime.rs
    pub(crate) fn start_send(&self, msg: IPCMessage) -> bool {
        match msg.ty().unwrap() {
            MessageType::Evaluate => self.eval_sender.try_send(msg).is_ok(),
            MessageType::Respond => self.respond_sender.unbounded_send(msg).is_ok(),
        }
=======
    fn new(slots: Arc<IPCSingleSlots>) -> Self {
        Self(Arc::new(IPCSenderSet { slots }))
    }

    pub(crate) fn send(&self, inbound: Inbound) -> Result<(), InboundSendError> {
        self.0.slots.send(inbound)
>>>>>>> /tmp/theirs_runtime.rs
    }
}

<<<<<<< /tmp/ours_runtime.rs
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
    /// The single pending inbound item. JS is synchronously blocked whenever an
    /// XHR is parked, so at most one item is ever outstanding.
    slot: Option<Inbound>,
    /// Waker for the async `poll_recv` waiter (the idle driver).
    recv_waker: Option<core::task::Waker>,
    sender_count: usize,
    closed: bool,
||||||| /tmp/base_runtime.rs
struct IPCReceivers {
    eval_receiver: Pin<Box<Receiver<IPCMessage>>>,
    respond_receiver: futures_channel::mpsc::UnboundedReceiver<IPCMessage>,
=======
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
>>>>>>> /tmp/theirs_runtime.rs
}

<<<<<<< /tmp/ours_runtime.rs
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
        let waker = state.recv_waker.take();
        drop(state);
        self.blocking_recv.notify_one();
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Inbound>> {
        let mut state = self.state.lock().unwrap();
        if let Some(value) = state.slot.take() {
            Poll::Ready(Some(value))
        } else if state.closed {
            Poll::Ready(None)
        } else {
            state.recv_waker = Some(cx.waker().clone());
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
||||||| /tmp/base_runtime.rs
impl IPCReceivers {
    pub fn recv_blocking(&mut self) -> Option<IPCMessage> {
        pollster::block_on(async {
            let Self {
                eval_receiver,
                respond_receiver,
            } = self;
            futures_util::select_biased! {
                // We need to always poll the respond receiver first. If the response is ready, quit immediately
                // before running any more callbacks
                respond_msg = respond_receiver.next().fuse() => {
                    respond_msg
                },
                eval_msg = eval_receiver.next().fuse() => {
                    eval_msg
                },
=======
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
>>>>>>> /tmp/theirs_runtime.rs
            }
<<<<<<< /tmp/ours_runtime.rs
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
            state.recv_waker.take()
        }
    }

    fn notify_closed(&self, waker: Option<Waker>) {
        self.blocking_recv.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
||||||| /tmp/base_runtime.rs
        })
=======
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
>>>>>>> /tmp/theirs_runtime.rs
    }
}

/// The runtime environment for communicating with JavaScript.
///
<<<<<<< /tmp/ours_runtime.rs
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages the single pending IPC slot.
||||||| /tmp/base_runtime.rs
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
=======
/// This struct sends commands to the main-thread driver and manages the single
/// pending inbound IPC slot.
>>>>>>> /tmp/theirs_runtime.rs
pub(crate) struct WryIPC {
<<<<<<< /tmp/ours_runtime.rs
    pub(crate) proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    slots: Arc<IPCSingleSlots>,
    webview_id: u64,
||||||| /tmp/base_runtime.rs
    pub(crate) proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    receivers: RwLock<IPCReceivers>,
=======
    slots: Arc<IPCSingleSlots>,
    commands: DriverCommandSender,
>>>>>>> /tmp/theirs_runtime.rs
}

impl WryIPC {
<<<<<<< /tmp/ours_runtime.rs
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(
        proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
        webview_id: u64,
    ) -> (Self, IPCSenders) {
        let slots = Arc::new(IPCSingleSlots::new());
        let senders = IPCSenders::new(slots.clone());
        let ipc = Self {
            proxy,
            slots,
            webview_id,
        };
        (ipc, senders)
    }

    /// The id of the webview this IPC channel belongs to.
    pub(crate) fn webview_id(&self) -> u64 {
        self.webview_id
||||||| /tmp/base_runtime.rs
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>) -> (Self, IPCSenders) {
        let (eval_sender, eval_receiver) = async_channel::unbounded();
        let (respond_sender, respond_receiver) = futures_channel::mpsc::unbounded();
        let senders = IPCSenders {
            eval_sender,
            respond_sender,
        };
        let receivers = RwLock::new(IPCReceivers {
            eval_receiver: Box::pin(eval_receiver),
            respond_receiver,
        });
        let ipc = Self { proxy, receivers };
        (ipc, senders)
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, id: u64, responder: OutboundIPCMessage) {
        (self.proxy)(WryBindgenEvent::ipc(id, responder));
=======
    /// Create a new runtime IPC pair and the driver command receiver.
    pub(crate) fn new() -> (Self, IPCSenders, DriverCommandReceiver) {
        let slots = Arc::new(IPCSingleSlots::new());
        let senders = IPCSenders::new(slots.clone());
        let command_queue = Arc::new(DriverCommandQueue::new());
        let commands = DriverCommandSender::new(command_queue.clone());
        let driver_commands = DriverCommandReceiver::new(command_queue);
        let ipc = Self { slots, commands };
        (ipc, senders, driver_commands)
>>>>>>> /tmp/theirs_runtime.rs
    }

<<<<<<< /tmp/ours_runtime.rs
    /// Send an IPC message to JavaScript for this webview.
    pub(crate) fn send_ipc(&self, message: IPCMessage) {
        (self.proxy)(WryBindgenEvent::ipc(self.webview_id, message));
    }
||||||| /tmp/base_runtime.rs
pub(crate) fn progress_js_with<O>(
    mut with_respond: impl for<'a> FnMut(DecodedData<'a>) -> O,
) -> Option<O> {
    let response = with_runtime(|runtime| runtime.ipc().receivers.write().recv_blocking())?;
    dispatch_inbound_message(&response, &mut with_respond)
}
=======
    /// Send a Rust-to-JS IPC payload through the main-thread driver.
    pub(crate) fn send_ipc(&self, message: OutboundIPCMessage) {
        self.commands.send(DriverCommand::SendIpc(message));
    }

    /// Ask the main thread to have JS park an XHR (the acquire half of the lock).
    pub(crate) fn send_acquire_lock(&self) {
        self.commands.send(DriverCommand::AcquireLock);
    }
>>>>>>> /tmp/theirs_runtime.rs

<<<<<<< /tmp/ours_runtime.rs
    /// Ask the main thread to have JS park an XHR (the acquire half of the lock).
    pub(crate) fn send_acquire_lock(&self) {
        (self.proxy)(WryBindgenEvent::acquire_lock(self.webview_id));
    }
||||||| /tmp/base_runtime.rs
pub async fn handle_callbacks() {
    let receiver = with_runtime(|runtime| runtime.ipc().receivers.read().eval_receiver.clone());
=======
    pub(crate) fn command_sender(&self) -> DriverCommandSender {
        self.commands.clone()
    }
>>>>>>> /tmp/theirs_runtime.rs

<<<<<<< /tmp/ours_runtime.rs
    pub(crate) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Inbound>> {
        self.slots.poll_recv(cx)
    }
}

impl Drop for WryIPC {
    fn drop(&mut self) {
        self.slots.close();
||||||| /tmp/base_runtime.rs
    while let Ok(response) = receiver.recv().await {
        dispatch_inbound_message(&response, &mut |_| unreachable!());
=======
    pub(crate) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Inbound>> {
        self.slots.poll_recv(cx)
>>>>>>> /tmp/theirs_runtime.rs
    }
}

<<<<<<< /tmp/ours_runtime.rs
pub(crate) fn progress_js_with<O>(
    with_respond: impl for<'a> FnMut(DecodedData<'a>) -> O,
||||||| /tmp/base_runtime.rs
fn dispatch_inbound_message<O>(
    response: &IPCMessage,
    with_respond: &mut impl for<'a> FnMut(DecodedData<'a>) -> O,
=======
impl Drop for WryIPC {
    fn drop(&mut self) {
        self.slots.close();
    }
}

pub(crate) fn progress_js_with<O>(
    with_respond: impl for<'a> FnMut(DecodedData<'a>) -> O,
>>>>>>> /tmp/theirs_runtime.rs
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
    let fn_id = u32::decode(data).expect("Failed to read fn_id");
    let response = match fn_id {
        // Call a registered Rust callback
        0 => {
            let key = u32::decode(data).unwrap();

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
            let result = (callback)(data, &mut encoder);
            // Flush any JS operations the callback queued before responding.
            crate::batch::force_flush();
            match result {
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

            let result = inventory::iter::<wry_bindgen_abi::JsExportSpec>()
                .find_map(|export| export.call_if_name(&export_name, data))
                .unwrap_or_else(|| panic!("Unknown export: {export_name}"));

            // Send response
            match result {
                Ok(encoded) => finish_respond_message(encoded),
                Err(err) => {
                    panic!("Export call failed: {err}");
                }
            }
        }
        _ => panic!("Unknown Rust callback function ID: {fn_id}"),
    };
    with_runtime(|runtime| runtime.ipc().send_ipc(response));
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

<<<<<<< /tmp/ours_runtime.rs
fn respond_encoder() -> crate::ipc::EncodedData {
    crate::ipc::EncodedData::default()
}

fn finish_respond_message(encoder: crate::ipc::EncodedData) -> IPCMessage {
    with_runtime(|runtime| runtime.finish_respond_message(encoder))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::MessageType;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ipc_message(message_type: MessageType) -> IPCMessage {
        crate::ipc::empty_message(message_type)
    }

    struct CountWaker {
        wakes: Arc<AtomicUsize>,
    }
||||||| /tmp/base_runtime.rs
/// Scopes the inbound-evaluate depth for the duration of a JS→Rust callback. The
/// depth is incremented on construction and decremented on drop.
struct InboundEvaluateGuard;

impl InboundEvaluateGuard {
    fn new() -> Self {
        with_runtime(|state| state.enter_inbound_evaluate());
        Self
    }
}

impl Drop for InboundEvaluateGuard {
    fn drop(&mut self) {
        with_runtime(|state| state.leave_inbound_evaluate());
    }
}

fn respond_encoder() -> crate::ipc::EncodedData {
    let mut encoder = crate::ipc::EncodedData::new();
    encoder.push_u8(MessageType::Respond as u8);
    encoder
}
=======
fn respond_encoder() -> crate::ipc::EncodedData {
    let mut encoder = crate::ipc::EncodedData::default();
    encoder.push_u8(MessageType::Respond as u8);
    encoder
}
>>>>>>> /tmp/theirs_runtime.rs

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
        let (ipc, senders) = WryIPC::new(Arc::new(|_| {}), 0);
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Pending));

        drop(senders);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Ready(None)));
    }

    #[test]
    fn ipc_sender_clone_lifetime() {
        let (ipc, sender) = WryIPC::new(Arc::new(|_| {}), 0);
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
        let (ipc, senders, _driver_commands) = WryIPC::new();
        let (waker, wakes) = counting_waker();
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Pending));

        drop(senders);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(matches!(ipc.poll_recv(&mut cx), Poll::Ready(None)));
    }

    #[test]
    fn ipc_sender_clone_lifetime() {
        let (ipc, sender, _driver_commands) = WryIPC::new();
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
