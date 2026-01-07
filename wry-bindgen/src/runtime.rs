//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use core::any::Any;
use core::cell::RefCell;
use core::pin::Pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::thread::ThreadId;

use alloc::boxed::Box;
use alloc::vec::Vec;
use async_channel::{Receiver, Sender};
use futures_channel::oneshot;
use futures_util::{FutureExt, StreamExt};
use once_cell::sync::OnceCell;
use spin::RwLock;

use slotmap::{DefaultKey, KeyData};

use crate::MessageType;
use crate::encode::BinaryDecode;
use crate::function::{
    CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback, THREAD_LOCAL_OBJECT_ENCODER,
};
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};

/// A task to be executed on the main thread with completion signaling and return value.
pub struct MainThreadTask {
    task: Box<dyn FnOnce() -> Box<dyn Any + Send + 'static> + Send + 'static>,
    completion: Option<oneshot::Sender<Box<dyn Any + Send + 'static>>>,
}

impl MainThreadTask {
    /// Create a new main thread task.
    pub fn new(
        task: Box<dyn FnOnce() -> Box<dyn Any + Send + 'static> + Send + 'static>,
        completion: oneshot::Sender<Box<dyn Any + Send + 'static>>,
    ) -> Self {
        Self {
            task,
            completion: Some(completion),
        }
    }

    /// Execute the task and signal completion with the return value.
    pub fn execute(mut self) {
        let result = (self.task)();
        if let Some(sender) = self.completion.take() {
            let _ = sender.send(result);
        }
    }
}

impl std::fmt::Debug for MainThreadTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainThreadTask")
            .field("task", &"<closure>")
            .finish()
    }
}

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
    /// Execute a closure on the main thread
    RunOnMainThread(MainThreadTask),
}

pub struct IPCSenders {
    eval_sender: Sender<IPCMessage>,
    respond_sender: futures_channel::mpsc::UnboundedSender<IPCMessage>,
}

impl IPCSenders {
    pub fn start_send(&self, msg: IPCMessage) {
        match msg.ty().unwrap() {
            MessageType::Evaluate => {
                self.eval_sender
                    .try_send(msg)
                    .expect("Failed to send evaluate message");
            }
            MessageType::Respond => {
                self.respond_sender
                    .unbounded_send(msg)
                    .expect("Failed to send respond message");
            }
        }
    }
}

struct IPCReceivers {
    eval_receiver: Pin<Box<Receiver<IPCMessage>>>,
    respond_receiver: futures_channel::mpsc::UnboundedReceiver<IPCMessage>,
}

impl IPCReceivers {
    pub async fn recv(&mut self) -> IPCMessage {
        let Self {
            eval_receiver,
            respond_receiver,
        } = self;
        futures_util::select_biased! {
            // We need to always poll the respond receiver first. If the response is ready, quit immediately
            // before running any more callbacks
            respond_msg = respond_receiver.next().fuse() => {
                respond_msg.expect("Failed to receive respond message")
            },
            eval_msg = eval_receiver.next().fuse() => {
                eval_msg.expect("Failed to receive evaluate message")
            },
        }
    }
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub struct WryRuntime {
    pub proxy: Box<dyn Fn(AppEvent) + Send + Sync>,
    pub(crate) queued_rust_calls: RwLock<Vec<IPCMessage>>,
    pub(crate) senders: OnceLock<IPCSenders>,
}

impl WryRuntime {
    /// Create a new runtime with the given event loop proxy.
    pub fn new(proxy: Box<dyn Fn(AppEvent) + Send + Sync>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            senders: OnceLock::new(),
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
        if let Some(senders) = self.senders.get() {
            senders.start_send(responder);
        } else {
            self.queued_rust_calls.write().push(responder);
        }
    }

    /// Set the sender for Rust calls and flush any queued calls.
    pub fn set_sender(&self, senders: IPCSenders) {
        self.senders
            .set(senders)
            .unwrap_or_else(|_| panic!("Sender already set"));
        let mut queued = self.queued_rust_calls.write();
        let senders = self.senders.get().expect("Senders not set");
        for call in queued.drain(..) {
            senders.start_send(call);
        }
    }
}

static RUNTIME: OnceCell<WryRuntime> = OnceCell::new();
static MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();

/// Check if the current thread is the main thread.
fn is_main_thread() -> bool {
    MAIN_THREAD_ID
        .get()
        .map(|id| *id == std::thread::current().id())
        .unwrap_or(false)
}

/// Set the event loop proxy for the runtime.
///
/// This must be called once before any JS operations are performed.
/// This also records the current thread as the main thread.
pub fn set_event_loop_proxy(proxy: Box<dyn Fn(AppEvent) + Send + Sync>) {
    MAIN_THREAD_ID
        .set(std::thread::current().id())
        .unwrap_or_else(|_| panic!("Main thread ID already set"));
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

/// Execute a closure on the main thread (winit event loop thread) and block until it completes,
/// returning the closure's result.
///
/// This function is useful for operations that must be performed on the main thread,
/// such as certain GUI operations or accessing thread-local state on the main thread.
///
/// # Panics
/// - Panics if called before the runtime is initialized (before `set_event_loop_proxy` is called)
/// - If the main thread exits before completing the task
///
/// # Note
/// If called from the main thread, the closure is executed immediately to avoid deadlock.
pub fn run_on_main_thread<T, F>(f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    // If we're already on the main thread, execute immediately to avoid deadlock
    if is_main_thread() {
        return f();
    }

    let (tx, rx) = oneshot::channel::<Box<dyn Any + Send + 'static>>();
    let task = MainThreadTask::new(
        Box::new(move || Box::new(f()) as Box<dyn Any + Send + 'static>),
        tx,
    );
    let runtime = get_runtime();
    (runtime.proxy)(AppEvent::RunOnMainThread(task));
    let result = pollster::block_on(rx).expect("Main thread did not complete the task");
    // SAFETY: We know the type is T because we boxed it as T above
    *result
        .downcast::<T>()
        .expect("Failed to downcast return value")
}

/// Get the runtime environment.
///
/// Panics if the runtime has not been initialized.
pub fn get_runtime() -> &'static WryRuntime {
    RUNTIME.get().expect("Event loop proxy not set")
}

thread_local! {
    static THREAD_LOCAL_RECEIVER: Rc<RefCell<IPCReceivers>> = Rc::new(RefCell::new({
        let runtime = get_runtime();
        let (eval_sender, eval_receiver) = async_channel::unbounded();
        let (respond_sender, respond_receiver) = futures_channel::mpsc::unbounded();
        let senders = IPCSenders {
            eval_sender,
            respond_sender,
        };
        runtime.set_sender(senders);
        IPCReceivers {
            eval_receiver: Box::pin(eval_receiver),
            respond_receiver,
        }
    }));
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
    progress_js_with(|mut data| {
        let response = R::decode(&mut data).expect("Failed to decode return value");
        assert!(
            data.is_empty(),
            "Extra data remaining after decoding response"
        );
        response
    })
    .await
}

#[allow(clippy::await_holding_refcell_ref)]
pub async fn progress_js_with<O>(with_respond: impl for<'a> Fn(DecodedData<'a>) -> O) -> Option<O> {
    let runtime = get_runtime();

    let response = THREAD_LOCAL_RECEIVER
        .with(|receiver| receiver.clone())
        .borrow_mut()
        .recv()
        .await;

    let decoder = response.decoded().expect("Failed to decode response");
    match decoder {
        DecodedVariant::Respond { data } => Some(with_respond(data)),
        DecodedVariant::Evaluate { mut data } => {
            handle_rust_callback(runtime, &mut data);
            None
        }
    }
}

pub async fn poll_callbacks() {
    let runtime = get_runtime();
    let receiver = THREAD_LOCAL_RECEIVER.with(|receiver| receiver.borrow().eval_receiver.clone());

    while let Ok(response) = receiver.recv().await {
        let decoder = response.decoded().expect("Failed to decode response");
        match decoder {
            DecodedVariant::Respond { .. } => unreachable!(),
            DecodedVariant::Evaluate { mut data } => {
                handle_rust_callback(runtime, &mut data);
            }
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
            let response = match result {
                Ok(encoded) => IPCMessage::new_respond(|encoder| {
                    encoder.extend(&encoded);
                }),
                Err(err) => {
                    panic!("Export call failed: {err}");
                }
            };
            runtime.js_response(response);
        }
        _ => todo!(),
    }
}
