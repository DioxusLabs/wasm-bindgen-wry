//! Runtime setup and event loop management.
//!
//! This module handles the connection between the Rust runtime and the
//! JavaScript environment via winit's event loop.

use core::any::Any;
use core::error::Error;
use core::fmt::Display;
use core::pin::Pin;
use std::sync::{Arc, mpsc};
use std::thread::ThreadId;

use alloc::boxed::Box;
use async_channel::{Receiver, Sender};
use futures_util::{FutureExt, StreamExt};
use spin::RwLock;

use crate::BinaryDecode;
use crate::batch::with_runtime;
use crate::function::{CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID, RustCallback};
use crate::ipc::MessageType;
use crate::ipc::{DecodedData, DecodedVariant, IPCMessage};
use crate::object_store::ObjectHandle;
use crate::object_store::remove_object;

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
    id: u64,
    event: AppEventVariant,
}

impl AppEvent {
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

    /// Create a new run-on-main-thread event.
    fn run_on_main_thread(id: u64, task: MainThreadTask) -> Self {
        Self {
            id,
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
    /// Execute a closure on the main thread
    RunOnMainThread(MainThreadTask),
}

#[derive(Clone)]
pub(crate) struct IPCSenders {
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
pub(crate) struct WryIPC {
    pub(crate) proxy: Arc<dyn Fn(AppEvent) + Send + Sync>,
    receivers: RwLock<IPCReceivers>,
    main_thread_id: ThreadId,
}

impl WryIPC {
    /// Create a new runtime with the given event loop proxy.
    pub(crate) fn new(
        proxy: Arc<dyn Fn(AppEvent) + Send + Sync>,
        main_thread_id: ThreadId,
    ) -> (Self, IPCSenders) {
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
        let ipc = Self {
            proxy,
            receivers,
            main_thread_id,
        };
        (ipc, senders)
    }

    /// Send a response back to JavaScript.
    pub(crate) fn js_response(&self, id: u64, responder: IPCMessage) {
        (self.proxy)(AppEvent::ipc(id, responder));
    }
}

/// Check if the current thread is the main thread.
fn is_main_thread() -> bool {
    with_runtime(|runtime| runtime.ipc().main_thread_id == std::thread::current().id())
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
    with_runtime(|runtime| {
        (runtime.ipc().proxy)(AppEvent::run_on_main_thread(runtime.webview_id(), task))
    });
    let result = rx.recv().expect("Main thread did not complete the task");
    *result
        .downcast::<T>()
        .expect("Failed to downcast return value")
}

pub(crate) fn progress_js_with<O>(
    with_respond: impl for<'a> Fn(DecodedData<'a>) -> O,
) -> Option<O> {
    let response = with_runtime(|runtime| runtime.ipc().receivers.write().recv_blocking());

    let decoder = response.decoded().expect("Failed to decode response");
    match decoder {
        DecodedVariant::Respond { data } => Some(with_respond(data)),
        DecodedVariant::Evaluate { mut data } => {
            handle_rust_callback(&mut data);
            None
        }
    }
}

pub async fn handle_callbacks() {
    let receiver = with_runtime(|runtime| runtime.ipc().receivers.read().eval_receiver.clone());

    while let Ok(response) = receiver.recv().await {
        let decoder = response.decoded().expect("Failed to decode response");
        match decoder {
            DecodedVariant::Respond { .. } => unreachable!(),
            DecodedVariant::Evaluate { mut data } => {
                handle_rust_callback(&mut data);
            }
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
