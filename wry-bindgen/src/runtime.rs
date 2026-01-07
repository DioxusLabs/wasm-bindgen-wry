//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use core::any::Any;
use core::error::Error;
use core::fmt::Display;
use core::pin::Pin;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread::ThreadId;

use alloc::boxed::Box;
use async_channel::{Receiver, Sender};
use futures_util::{FutureExt, StreamExt};
use once_cell::sync::OnceCell;
use spin::RwLock;

use slotmap::{DefaultKey, KeyData};

use crate::function::{
    CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback, THREAD_LOCAL_OBJECT_ENCODER,
};
use crate::ipc::MessageType;
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};
use crate::wry::WryBindgen;

/// A task to be executed on the main thread with completion signaling and return value.
pub struct MainThreadTask {
    task: Box<dyn FnOnce() -> Box<dyn Any + Send + 'static> + Send + 'static>,
    completion: Option<mpsc::SyncSender<Box<dyn Any + Send + 'static>>>,
}

impl MainThreadTask {
    /// Create a new main thread task.
    pub fn new(
        task: Box<dyn FnOnce() -> Box<dyn Any + Send + 'static> + Send + 'static>,
        completion: mpsc::SyncSender<Box<dyn Any + Send + 'static>>,
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
pub struct AppEvent {
    event: AppEventVariant,
}

impl AppEvent {
    /// Create a new IPC event.
    pub(crate) fn ipc(msg: IPCMessage) -> Self {
        Self {
            event: AppEventVariant::Ipc(msg),
        }
    }

    /// Create a new webview loaded event.
    pub(crate) fn webview_loaded() -> Self {
        Self {
            event: AppEventVariant::WebviewLoaded,
        }
    }

    /// Create a new shutdown event with the given status code.
    pub(crate) fn shutdown(status: i32) -> Self {
        Self {
            event: AppEventVariant::Shutdown(status),
        }
    }

    /// Create a new run-on-main-thread event.
    fn run_on_main_thread(task: MainThreadTask) -> Self {
        Self {
            event: AppEventVariant::RunOnMainThread(task),
        }
    }

    /// Consume the event and return the inner variant.
    pub(crate) fn into_variant(self) -> AppEventVariant {
        self.event
    }
}

#[derive(Debug)]
pub(crate) enum AppEventVariant {
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
    pub(crate) fn start_send(&self, msg: IPCMessage) {
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
    pub fn recv_blocking(&mut self) -> IPCMessage {
        pollster::block_on(async {
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
        })
    }
}

/// The runtime environment for communicating with JavaScript.
///
/// This struct holds the event loop proxy for sending messages to the
/// WebView and manages queued Rust calls.
pub struct WryRuntime {
    pub proxy: Box<dyn Fn(AppEvent) + Send + Sync>,
    pub(crate) senders: IPCSenders,
    receivers: RwLock<IPCReceivers>,
}

impl WryRuntime {
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(proxy: Box<dyn Fn(AppEvent) + Send + Sync>) -> Self {
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
        Self {
            proxy,
            senders,
            receivers,
        }
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, responder: IPCMessage) {
        (self.proxy)(AppEvent::ipc(responder));
    }

    /// Request the application to shut down with a status code.
    pub fn shutdown(&self, status: i32) {
        (self.proxy)(AppEvent::shutdown(status));
    }

    /// Queue a Rust call from JavaScript.
    pub(crate) fn queue_rust_call(&self, responder: IPCMessage) {
        self.senders.start_send(responder);
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

/// Error indicating that the runtime has already been started.
#[derive(Debug)]
pub struct AlreadyStartedError;

impl Display for AlreadyStartedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "The runtime has already been started")
    }
}

impl Error for AlreadyStartedError {}

/// Start the application thread with the given event loop proxy
pub fn start_app<F>(
    event_loop_proxy: impl Fn(AppEvent) + Send + Sync + 'static,
    app: impl FnOnce() -> F + Send + 'static,
    start_async_runtime: impl FnOnce(Pin<Box<dyn Future<Output = ()>>>) + Send + 'static,
) -> Result<WryBindgen, AlreadyStartedError>
where
    F: core::future::Future<Output = ()> + 'static,
{
    // Record the main thread ID before doing anything else
    MAIN_THREAD_ID
        .set(std::thread::current().id())
        .unwrap_or_else(|_| panic!("Main thread ID already set"));

    let event_loop_proxy = Box::new(event_loop_proxy) as Box<dyn Fn(AppEvent) + Send + Sync>;
    if RUNTIME.set(WryRuntime::new(event_loop_proxy)).is_err() {
        eprintln!("start_app can only be called once per process. Exiting.");
        return Err(AlreadyStartedError);
    }
    // Spawn the app thread with panic handling - if the app panics, shut down the webview
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
        let run = || {
            let run_app = app();
            let wait_for_events = poll_callbacks();

            start_async_runtime(Box::pin(async move {
                futures_util::select! {
                    _ = run_app.fuse() => {},
                    _ = wait_for_events.fuse() => {},
                }
            }));
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        let status = if let Err(panic_info) = result {
            eprintln!("App thread panicked, shutting down webview");
            // Try to print panic info
            if let Some(s) = panic_info.downcast_ref::<&str>() {
                eprintln!("Panic message: {s}");
            } else if let Some(s) = panic_info.downcast_ref::<alloc::string::String>() {
                eprintln!("Panic message: {s}");
            }
            1 // Exit with error status on panic
        } else {
            0 // Exit with success status on normal completion
        };
        shutdown(status);
    }).expect("Failed to spawn app thread");

    Ok(WryBindgen::new())
}

/// Request the application to shut down with a status code.
///
/// This sends a shutdown event through the event loop, which will cause
/// the webview to close and the application to exit with the given status code.
pub(crate) fn shutdown(status: i32) {
    get_runtime().shutdown(status);
}

/// Execute a closure on the main thread (winit event loop thread) and block until it completes,
/// returning the closure's result.
///
/// This function is useful for operations that must be performed on the main thread,
/// such as certain GUI operations or accessing thread-local state on the main thread.
///
/// # Panics
/// - Panics if called before the runtime is initialized (before `start_app` is called)
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

    let (tx, rx) = mpsc::sync_channel::<Box<dyn Any + Send + 'static>>(1);
    let task = MainThreadTask::new(
        Box::new(move || Box::new(f()) as Box<dyn Any + Send + 'static>),
        tx,
    );
    let runtime = get_runtime();
    (runtime.proxy)(AppEvent::run_on_main_thread(task));
    let result = rx.recv().expect("Main thread did not complete the task");
    *result
        .downcast::<T>()
        .expect("Failed to downcast return value")
}

/// Get the runtime environment.
///
/// Panics if the runtime has not been initialized.
pub(crate) fn get_runtime() -> &'static WryRuntime {
    RUNTIME.get().expect("Event loop proxy not set")
}

pub(crate) fn progress_js_with<O>(
    with_respond: impl for<'a> Fn(DecodedData<'a>) -> O,
) -> Option<O> {
    let runtime = get_runtime();

    let response = get_runtime().receivers.write().recv_blocking();

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
    let receiver = get_runtime().receivers.read().eval_receiver.clone();

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
