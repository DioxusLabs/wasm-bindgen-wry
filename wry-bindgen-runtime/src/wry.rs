//! Per-webview wry-bindgen integration for existing wry applications.
//!
//! A [`WryBindgen`] session is split into an app-thread runtime endpoint and a
//! main-thread driver. Create one session for each webview/JavaScript
//! realm.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use base64::Engine;
use core::cell::RefCell;
use core::future::poll_fn;
<<<<<<< /tmp/ours_wry.rs
use core::pin::Pin;
use core::sync::atomic::AtomicU64;
use core::task::Poll;
use std::collections::HashMap;
use std::sync::Arc;
||||||| /tmp/base_wry.rs
use core::pin::{Pin, pin};
use futures_util::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
=======
use core::pin::Pin;
use core::task::Poll;
>>>>>>> /tmp/theirs_wry.rs

use http::Response;

use crate::batch::{Runtime, in_runtime};
use crate::function_registry::FUNCTION_REGISTRY;
<<<<<<< /tmp/ours_wry.rs
use crate::ipc::{IPCMessage, decode_data};
use crate::runtime::{
    AppEventVariant, IPCSenders, Inbound, InboundSendError, WryIPC, dispatch_inbound_message,
};

pub use crate::runtime::WryBindgenEvent;

pub trait ImplWryBindgenResponder {
    fn respond(self: Box<Self>, response: Response<Vec<u8>>);
}

/// Responder for wry-bindgen protocol requests.
pub struct WryBindgenResponder {
    respond: Box<dyn ImplWryBindgenResponder>,
||||||| /tmp/base_wry.rs
use crate::ipc::{DecodedVariant, IPCMessage, MessageType, OutboundIPCMessage, decode_data};
use crate::runtime::{AppEventVariant, IPCSenders, WryIPC, handle_callbacks};

pub use crate::runtime::WryBindgenEvent;

pub trait ImplWryBindgenResponder {
    fn respond(self: Box<Self>, response: Response<Vec<u8>>);
}

/// Responder for wry-bindgen protocol requests.
pub struct WryBindgenResponder {
    respond: Box<dyn ImplWryBindgenResponder>,
=======
use crate::ipc::{IPCMessage, OutboundIPCMessage, decode_data};
use crate::runtime::{
    DriverCommand, DriverCommandReceiver, DriverCommandSender, DriverCommandWeakSender, IPCSenders,
    Inbound, InboundSendError, WryIPC, dispatch_inbound_message,
};

struct WryBindgenResponder {
    respond: Box<dyn FnOnce(Response<Vec<u8>>)>,
>>>>>>> /tmp/theirs_wry.rs
}

impl<F> From<F> for WryBindgenResponder
where
    F: FnOnce(Response<Vec<u8>>) + 'static,
{
    fn from(respond: F) -> Self {
        Self {
            respond: Box::new(respond),
        }
    }
}

impl WryBindgenResponder {
    fn respond(self, response: Response<Vec<u8>>) {
        (self.respond)(response);
    }

    fn respond_ipc(self, response: IPCMessage) {
        let body = response.data();
        // Encode as base64 - sync XMLHttpRequest cannot use responseType="arraybuffer"
        let engine = base64::engine::general_purpose::STANDARD;
        let body_base64 = engine.encode(body);
        self.respond(
            http::Response::builder()
                .status(200)
                .header("Content-Type", "text/plain")
                .body(body_base64.into_bytes())
                .expect("Failed to build response"),
        );
    }
}

/// Decode request data from the dioxus-data header.
fn decode_request_data(request: &http::Request<Vec<u8>>) -> Option<IPCMessage> {
    if let Some(header_value) = request.headers().get("dioxus-data") {
        return decode_data(header_value.as_bytes());
    }
    None
}

/// Tracks the loading state of the webview.
enum WebviewLoadingState {
<<<<<<< /tmp/ours_wry.rs
    /// Webview is still loading. The lock protocol permits at most one Rust IPC
    /// message to be waiting for load, though normally this remains empty
    /// because user code cannot run until the lock can be acquired.
    Pending {
        pending_ipc: Option<IPCMessage>,
        acquire_lock: bool,
    },
||||||| /tmp/base_wry.rs
    /// Webview is still loading, messages are queued.
    Pending { queued: Vec<OutboundIPCMessage> },
=======
    /// Webview is still loading. The lock protocol permits at most one Rust IPC
    /// message to be waiting for load, though normally this remains empty
    /// because user code cannot run until the lock can be acquired.
    Pending {
        pending_ipc: Option<OutboundIPCMessage>,
        acquire_lock: bool,
    },
>>>>>>> /tmp/theirs_wry.rs
    /// Webview is loaded and ready.
    Loaded,
}

impl Default for WebviewLoadingState {
    fn default() -> Self {
        WebviewLoadingState::Pending {
            pending_ipc: None,
            acquire_lock: false,
        }
    }
}

/// Shared state for one webview instance.
struct WebviewState {
    /// Protocol message routing for this webview.
    messages: WebviewMessageLayer,
    // The state of the webview. Either loading (with queued messages) or loaded.
    loading_state: WebviewLoadingState,
}

/// Transport-owned IPC routing state for one webview.
///
/// Under strict synchronous ping-pong:
///
/// - At most one JS XHR is suspended at any moment (JS blocks on each XHR
///   before it can send the next one), so the responder lives in a single
///   `current_xhr` slot.
/// - Rust->JS calls are only delivered through that suspended XHR, and JS
///   replies by parking the next XHR on the same response path.
struct WebviewMessageLayer {
    current_xhr: Option<WryBindgenResponder>,
    /// The sender used to forward decoded IPC messages to the Rust runtime.
    sender: IPCSenders,
}

impl WebviewState {
    /// Create a new webview state.
    fn new(sender: IPCSenders) -> Self {
        Self {
            messages: WebviewMessageLayer::new(sender),
            loading_state: WebviewLoadingState::default(),
        }
    }

    fn handle_driver_command(&mut self, command: DriverCommand) -> DriverAction {
        match command {
            DriverCommand::AcquireLock => self.handle_acquire_lock(),
            DriverCommand::SendIpc(ipc_msg) => {
                self.handle_ipc_message(ipc_msg);
                DriverAction::None
            }
            DriverCommand::ReleaseLock => {
                self.messages.release_lock();
                DriverAction::None
            }
        }
    }

<<<<<<< /tmp/ours_wry.rs
impl WebviewMessageLayer {
    fn new(sender: IPCSenders) -> Self {
        Self {
            current_xhr: None,
            sender,
||||||| /tmp/base_wry.rs
impl WebviewMessageLayer {
    fn new(sender: IPCSenders) -> Self {
        Self {
            current_xhr: None,
            rust_eval_stack: Vec::new(),
            sender,
=======
    fn handle_ipc_message(&mut self, ipc_msg: OutboundIPCMessage) {
        if let WebviewLoadingState::Pending { pending_ipc, .. } = &mut self.loading_state {
            assert!(
                pending_ipc.replace(ipc_msg).is_none(),
                "multiple Rust IPC messages queued before webview load"
            );
            return;
>>>>>>> /tmp/theirs_wry.rs
        }

<<<<<<< /tmp/ours_wry.rs
    fn receive_js_message(&mut self, msg: IPCMessage, responder: WryBindgenResponder) {
        self.park_and_forward(responder, Inbound::Message(msg));
||||||| /tmp/base_wry.rs
    fn receive_js_message(&mut self, msg: IPCMessage, responder: WryBindgenResponder) {
        let msg_type = msg.ty().unwrap();

        // JS can only send a message when it isn't blocked on an existing XHR,
        // so `current_xhr` must be empty at this point.
        if self.current_xhr.is_some() {
            responder.respond(error_response());
            return;
        }

        let top_level_responder = match msg_type {
            // New call from JS — park the XHR. Rust will reply via either a
            // Respond (the answer) or an Evaluate (a nested Rust→JS call
            // delivered through the suspended XHR).
            MessageType::Evaluate => {
                self.current_xhr = Some(responder);
                None
            }
            // Response from JS closes the most recent Rust Evaluate frame.
            // Nested frames hand the new XHR off as the next wait point;
            // top-level frames have no parent so we close the chain with a
            // blank response after Rust has accepted the Respond.
            MessageType::Respond => match self.rust_eval_stack.pop() {
                Some(RustEvalKind::Nested) => {
                    self.current_xhr = Some(responder);
                    None
                }
                Some(RustEvalKind::TopLevel) => Some(responder),
                None => {
                    responder.respond(error_response());
                    return;
                }
            },
        };

        if self.sender.start_send(msg) {
            if let Some(responder) = top_level_responder {
                responder.respond(blank_response());
            }
        } else if let Some(responder) = top_level_responder {
            responder.respond(error_response());
        } else if let Some(responder) = self.current_xhr.take() {
            responder.respond(error_response());
        }
=======
        self.messages.receive_rust_message(ipc_msg);
    }

    fn handle_acquire_lock(&mut self) -> DriverAction {
        if let WebviewLoadingState::Pending { acquire_lock, .. } = &mut self.loading_state {
            *acquire_lock = true;
            return DriverAction::None;
        }

        DriverAction::RequestJsLock
    }

    fn mark_loaded(&mut self) -> bool {
        if let WebviewLoadingState::Pending {
            pending_ipc,
            acquire_lock,
        } = std::mem::replace(&mut self.loading_state, WebviewLoadingState::Loaded)
        {
            if let Some(msg) = pending_ipc {
                self.messages.receive_rust_message(msg);
            }
            return acquire_lock;
        }

        false
>>>>>>> /tmp/theirs_wry.rs
    }
}

<<<<<<< /tmp/ours_wry.rs
    fn receive_lock_request(&mut self, responder: WryBindgenResponder) {
        self.park_and_forward(responder, Inbound::LockReady);
    }
||||||| /tmp/base_wry.rs
    fn receive_rust_message(&mut self, ipc_msg: OutboundIPCMessage) -> Option<IPCMessage> {
        let ty = ipc_msg.message.ty().unwrap();
        let top_level = ipc_msg.top_level;
        let message = ipc_msg.message;
=======
enum DriverAction {
    None,
    RequestJsLock,
}
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
    fn park_and_forward(&mut self, responder: WryBindgenResponder, inbound: Inbound) {
        assert!(
            self.current_xhr.is_none(),
            "JS parked a new XHR while another JS XHR is waiting for Rust"
        );
        self.current_xhr = Some(responder);
        match self.sender.send(inbound) {
            Ok(()) => {}
            Err(InboundSendError::Closed) => {
                let responder = self.take_parked_xhr();
                responder.respond(error_response());
            }
            Err(InboundSendError::Occupied) => {
                panic!("inbound IPC slot occupied while parking a JS XHR")
||||||| /tmp/base_wry.rs
        match ty {
            MessageType::Respond => {
                let responder = self
                    .current_xhr
                    .take()
                    .expect("Rust Respond with no suspended JS XHR to reply to");
                responder.respond_ipc(message);
                None
            }
            // The runtime tells us whether this Evaluate is a fresh top-level
            // call or a nested response inside a callback. We must not infer it
            // from `current_xhr`: a callback XHR can already be parked (awaiting
            // the app future to pick it up) when the app future emits an
            // unrelated top-level eval, which would otherwise be misdelivered as
            // that callback's response.
            MessageType::Evaluate if top_level => {
                // Top-level: caller delivers via `evaluate_script`. JS, if it is
                // currently blocked on a parked callback XHR, will run this only
                // once that callback's JS→Rust chain fully resolves.
                self.rust_eval_stack.push(RustEvalKind::TopLevel);
                Some(message)
            }
            MessageType::Evaluate => {
                // Nested: deliver as the response to the parked JS XHR.
                let responder = self
                    .current_xhr
                    .take()
                    .expect("Nested Rust Evaluate with no suspended JS XHR to reply to");
                responder.respond_ipc(message);
                self.rust_eval_stack.push(RustEvalKind::Nested);
                None
=======
impl DriverAction {
    fn run(self, evaluate_script: &mut impl FnMut(&str)) {
        match self {
            DriverAction::None => {}
            DriverAction::RequestJsLock => {
                evaluate_script("window.__wry_acquire_handler_lock()");
>>>>>>> /tmp/theirs_wry.rs
            }
        }
    }
<<<<<<< /tmp/ours_wry.rs
||||||| /tmp/base_wry.rs
}

fn unique_id() -> u64 {
    use core::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
=======
}

impl WebviewMessageLayer {
    fn new(sender: IPCSenders) -> Self {
        Self {
            current_xhr: None,
            sender,
        }
    }
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
    fn receive_rust_message(&mut self, ipc_msg: IPCMessage) {
        // Deliver as the response to the parked JS XHR. This is the only
        // Rust->JS payload path; `evaluate_script` is reserved for asking JS to
        // acquire this lock.
        let responder = self.take_parked_xhr();
        responder.respond_ipc(ipc_msg);
    }
||||||| /tmp/base_wry.rs
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// A webview future that has a reserved id for use with wry-bindgen.
///
/// This struct is `Send` and can be moved to a spawned thread.
/// Use `into_future()` to get the actual future to poll.
pub struct PreparedApp {
    id: u64,
    future: Box<dyn FnOnce() -> Pin<Box<dyn core::future::Future<Output = ()> + 'static>> + Send>,
}
=======
    fn receive_js_message(&mut self, msg: IPCMessage, responder: WryBindgenResponder) {
        self.park_and_forward(responder, Inbound::Message(msg));
    }

    fn receive_lock_request(&mut self, responder: WryBindgenResponder) {
        self.park_and_forward(responder, Inbound::LockReady);
    }

    fn park_and_forward(&mut self, responder: WryBindgenResponder, inbound: Inbound) {
        assert!(
            self.current_xhr.is_none(),
            "JS parked a new XHR while another JS XHR is waiting for Rust"
        );
        self.current_xhr = Some(responder);
        match self.sender.send(inbound) {
            Ok(()) => {}
            Err(InboundSendError::Closed) => {
                let responder = self.take_parked_xhr();
                responder.respond(error_response());
            }
            Err(InboundSendError::Occupied) => {
                panic!("inbound IPC slot occupied while parking a JS XHR")
            }
        }
    }
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
    fn release_lock(&mut self) {
        let responder = self.take_parked_xhr();
        responder.respond(blank_response());
||||||| /tmp/base_wry.rs
impl PreparedApp {
    /// Get the unique id of this PreparedApp.
    pub fn id(&self) -> u64 {
        self.id
=======
    fn receive_rust_message(&mut self, ipc_msg: OutboundIPCMessage) {
        // Deliver as the response to the parked JS XHR. This is the only
        // Rust->JS payload path; `evaluate_script` is reserved for asking JS to
        // acquire this lock.
        let responder = self.take_parked_xhr();
        responder.respond_ipc(ipc_msg.message);
>>>>>>> /tmp/theirs_wry.rs
    }

<<<<<<< /tmp/ours_wry.rs
    /// Take the JS XHR currently parked in this layer. Every caller runs only
    /// while Rust holds the lock, so an XHR is always suspended here.
    fn take_parked_xhr(&mut self) -> WryBindgenResponder {
        self.current_xhr.take().unwrap()
||||||| /tmp/base_wry.rs
    /// Get the inner future of this PreparedApp.
    pub fn into_future(self) -> Pin<Box<dyn core::future::Future<Output = ()> + 'static>> {
        (self.future)()
=======
    fn release_lock(&mut self) {
        let responder = self.take_parked_xhr();
        responder.respond(blank_response());
    }

    /// Take the JS XHR currently parked in this layer. Every caller runs only
    /// while Rust holds the lock, so an XHR is always suspended here.
    fn take_parked_xhr(&mut self) -> WryBindgenResponder {
        self.current_xhr.take().unwrap()
>>>>>>> /tmp/theirs_wry.rs
    }
}

/// Protocol handler for one webview session.
///
/// This struct is not `Send` because it holds main-thread webview state.
pub struct ProtocolHandler {
    webview: Rc<RefCell<WebviewState>>,
    driver_commands: DriverCommandWeakSender,
}

impl ProtocolHandler {
    /// Create a protocol handler closure suitable for `WebViewBuilder::with_asynchronous_custom_protocol`.
    ///
    /// The returned closure handles this subset of "{protocol}://" requests:
    /// - "/__wbg__/initialized" - signals webview loaded
    /// - "/__wbg__/snippets/{path}" - serves inline JS modules
    /// - "/__wbg__/init.js" - serves the initialization script
    /// - "/__wbg__/handler" - main IPC endpoint
    ///
    /// # Arguments
    /// * `protocol` - The protocol scheme (e.g., "wry")
    pub fn handle_request<R>(
        &self,
        protocol: &str,
        request: &http::Request<Vec<u8>>,
        responder: R,
    ) -> Option<R>
    where
        R: FnOnce(Response<Vec<u8>>) + 'static,
    {
        let webviews = &self.webview;

        let protocol_prefix = format!("{protocol}://index.html");
        let android_prefix = format!("https://{protocol}.index.html");
        let windows_prefix = format!("http://{protocol}.index.html");

        let uri = request.uri().to_string();
        let real_path = uri
            .strip_prefix(&protocol_prefix)
            .or_else(|| uri.strip_prefix(&windows_prefix))
            .or_else(|| uri.strip_prefix(&android_prefix))
            .unwrap_or(&uri);
        let real_path = real_path.trim_matches('/');

        let Some(path_without_wbg) = real_path.strip_prefix("__wbg__/") else {
            // Not a wry-bindgen request - let the caller handle it
            return Some(responder);
        };

        // Serve inline_js modules from __wbg__/snippets/
        if let Some(path_without_snippets) = path_without_wbg.strip_prefix("snippets/") {
            let responder = WryBindgenResponder::from(responder);
            if let Some(content) = FUNCTION_REGISTRY.get_module(path_without_snippets) {
                responder.respond(module_response(content));
                return None;
            }
            responder.respond(not_found_response());
            return None;
        }

        if path_without_wbg == "init.js" {
            let responder = WryBindgenResponder::from(responder);
            responder.respond(module_response(&init_script()));
            return None;
        }

        if path_without_wbg == "initialized" {
            let acquire_lock = webviews.borrow_mut().mark_loaded();
            if acquire_lock {
                self.driver_commands.send(DriverCommand::AcquireLock);
            }
            let responder = WryBindgenResponder::from(responder);
            responder.respond(blank_response());
            return None;
        }

        // Js sent us either an Evaluate or Respond message
        if path_without_wbg == "handler" {
            let responder = WryBindgenResponder::from(responder);
            let mut webview_state = webviews.borrow_mut();
            if request.headers().get("wry-bindgen-lock").is_some() {
                webview_state.messages.receive_lock_request(responder);
                return None;
<<<<<<< /tmp/ours_wry.rs
            };
            if request.headers().get("wry-bindgen-lock").is_some() {
                webview_state.messages.receive_lock_request(responder);
                return None;
            }
||||||| /tmp/base_wry.rs
            };
=======
            }
>>>>>>> /tmp/theirs_wry.rs
            let Some(msg) = decode_request_data(request) else {
                responder.respond(error_response());
                return None;
            };
            webview_state.messages.receive_js_message(msg, responder);
            return None;
        }

        Some(responder)
    }
}

/// Get the initialization script that must be evaluated in the webview.
///
/// This script sets up the JavaScript function registry and IPC infrastructure.
fn init_script() -> String {
    /// The script you need to include in the initialization of your webview.
    const INITIALIZATION_SCRIPT: &str = include_str!("./js/main.js");
    let collect_functions = FUNCTION_REGISTRY.script();
    format!("{INITIALIZATION_SCRIPT}\n{collect_functions}")
}

/// Per-webview wry-bindgen session.
///
/// Each session owns one JavaScript realm. Split it into a runtime endpoint for
/// the app thread and a driver that must be polled on the webview thread.
pub struct WryBindgen {
<<<<<<< /tmp/ours_wry.rs
    event_loop_proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    max_id: AtomicU64,
    // State that is unique to each webview
    webview: Rc<RefCell<HashMap<u64, WebviewState>>>,
||||||| /tmp/base_wry.rs
    event_loop_proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    // State that is unique to each webview
    webview: Rc<RefCell<HashMap<u64, WebviewState>>>,
=======
    webview: Rc<RefCell<WebviewState>>,
    ipc: WryIPC,
    driver_commands: DriverCommandReceiver,
    weak_driver_commands: DriverCommandWeakSender,
>>>>>>> /tmp/theirs_wry.rs
}

impl WryBindgen {
    /// Create a new per-webview session.
    pub fn new() -> Self {
        let (ipc, senders, driver_commands) = WryIPC::new();
        let weak_driver_commands = ipc.command_sender().downgrade();
        Self {
<<<<<<< /tmp/ours_wry.rs
            event_loop_proxy: Arc::new(event_loop_proxy),
            max_id: AtomicU64::new(0),
            webview: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Start the application thread with the given event loop proxy.
    ///
    /// Returns a tuple of:
    /// - `PreparedApp`: The app future, which is `Send` and can be moved to a spawned thread
    /// - `ProtocolHandlerFactory`: Factory for creating the protocol handler (not `Send`, use on main thread)
    pub fn app_builder<'a>(&'a self) -> AppBuilder<'a> {
        let event_loop_proxy = self.event_loop_proxy.clone();
        let id = self
            .max_id
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        let (ipc, senders) = WryIPC::new(event_loop_proxy, id);
        self.webview.borrow_mut().insert(
            id,
            WebviewState::new(senders, |_| {
                unreachable!("evaluate_script will only be used after spawning the app")
            }),
        );

        AppBuilder { bindgen: self, ipc }
||||||| /tmp/base_wry.rs
            event_loop_proxy: Arc::new(event_loop_proxy),
            webview: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Start the application thread with the given event loop proxy.
    ///
    /// Returns a tuple of:
    /// - `PreparedApp`: The app future, which is `Send` and can be moved to a spawned thread
    /// - `ProtocolHandlerFactory`: Factory for creating the protocol handler (not `Send`, use on main thread)
    pub fn app_builder<'a>(&'a self) -> AppBuilder<'a> {
        let event_loop_proxy = self.event_loop_proxy.clone();
        let webview_id = unique_id();
        let (ipc, senders) = WryIPC::new(event_loop_proxy);
        self.webview.borrow_mut().insert(
            webview_id,
            WebviewState::new(senders, |_| {
                unreachable!("evaluate_script will only be used after spawning the app")
            }),
        );

        AppBuilder {
            webview_id,
            bindgen: self,
            ipc,
        }
=======
            webview: Rc::new(RefCell::new(WebviewState::new(senders))),
            ipc,
            driver_commands,
            weak_driver_commands,
        }
>>>>>>> /tmp/theirs_wry.rs
    }

<<<<<<< /tmp/ours_wry.rs
    /// Handle a user event from the event loop.
    ///
    /// This should be called from your ApplicationHandler::user_event implementation.
    /// Returns `Some(exit_code)` if the application should shut down with that exit code.
    ///
    /// # Arguments
    /// * `event` - The AppEvent to handle
    /// * `webview` - Reference to the webview for script evaluation
    pub fn handle_user_event(&self, event: WryBindgenEvent) {
        let id = event.id();
        match event.into_variant() {
            // The rust thread sent us an IPCMessage to send to JS
            AppEventVariant::Ipc(ipc_msg) => self.handle_ipc_message(id, ipc_msg),
            AppEventVariant::WebviewLoaded => {
                let mut state = self.webview.borrow_mut();
                let Some(webview_state) = state.get_mut(&id) else {
                    return;
                };
                if let WebviewLoadingState::Pending {
                    pending_ipc,
                    acquire_lock,
                } = std::mem::replace(
                    &mut webview_state.loading_state,
                    WebviewLoadingState::Loaded,
                ) {
                    if let Some(msg) = pending_ipc {
                        self.immediately_handle_ipc_message(webview_state, msg);
                    }
                    if acquire_lock {
                        self.request_js_lock(webview_state);
                    }
                }
            }
            AppEventVariant::HandlerLock { acquire: true } => {
                let mut state = self.webview.borrow_mut();
                let Some(webview_state) = state.get_mut(&id) else {
                    return;
                };
                if let WebviewLoadingState::Pending { acquire_lock, .. } =
                    &mut webview_state.loading_state
                {
                    *acquire_lock = true;
                    return;
                }
                self.request_js_lock(webview_state);
            }
            AppEventVariant::HandlerLock { acquire: false } => {
                let mut state = self.webview.borrow_mut();
                let Some(webview_state) = state.get_mut(&id) else {
                    return;
                };
                webview_state.messages.release_lock();
            }
||||||| /tmp/base_wry.rs
    /// Handle a user event from the event loop.
    ///
    /// This should be called from your ApplicationHandler::user_event implementation.
    /// Returns `Some(exit_code)` if the application should shut down with that exit code.
    ///
    /// # Arguments
    /// * `event` - The AppEvent to handle
    /// * `webview` - Reference to the webview for script evaluation
    pub fn handle_user_event(&self, event: WryBindgenEvent) {
        let id = event.id();
        match event.into_variant() {
            // The rust thread sent us an IPCMessage to send to JS
            AppEventVariant::Ipc(ipc_msg) => self.handle_ipc_message(id, ipc_msg),
            AppEventVariant::WebviewLoaded => {
                let mut state = self.webview.borrow_mut();
                let Some(webview_state) = state.get_mut(&id) else {
                    return;
                };
                if let WebviewLoadingState::Pending { queued } = std::mem::replace(
                    &mut webview_state.loading_state,
                    WebviewLoadingState::Loaded,
                ) {
                    for msg in queued {
                        self.immediately_handle_ipc_message(webview_state, msg);
                    }
                }
            }
=======
    /// Get the protocol handler for this webview.
    pub fn protocol_handler(&self) -> ProtocolHandler {
        ProtocolHandler {
            webview: self.webview.clone(),
            driver_commands: self.weak_driver_commands.clone(),
>>>>>>> /tmp/theirs_wry.rs
        }
    }

<<<<<<< /tmp/ours_wry.rs
    fn handle_ipc_message(&self, id: u64, ipc_msg: IPCMessage) {
        let mut state = self.webview.borrow_mut();
        let Some(webview_state) = state.get_mut(&id) else {
            return;
        };
        if let WebviewLoadingState::Pending { pending_ipc, .. } = &mut webview_state.loading_state {
            assert!(
                pending_ipc.replace(ipc_msg).is_none(),
                "multiple Rust IPC messages queued before webview load"
            );
            return;
        }

        self.immediately_handle_ipc_message(webview_state, ipc_msg)
||||||| /tmp/base_wry.rs
    fn handle_ipc_message(&self, id: u64, ipc_msg: OutboundIPCMessage) {
        let mut state = self.webview.borrow_mut();
        let Some(webview_state) = state.get_mut(&id) else {
            return;
        };
        if let WebviewLoadingState::Pending { queued } = &mut webview_state.loading_state {
            queued.push(ipc_msg);
            return;
        }

        self.immediately_handle_ipc_message(webview_state, ipc_msg)
=======
    /// Split the session into an app-thread runtime endpoint and a main-thread driver.
    pub fn split(self) -> (WryBindgenRuntime, WryBindgenDriver) {
        (
            WryBindgenRuntime { ipc: self.ipc },
            WryBindgenDriver {
                webview: self.webview,
                commands: self.driver_commands,
            },
        )
>>>>>>> /tmp/theirs_wry.rs
    }
}

<<<<<<< /tmp/ours_wry.rs
    fn request_js_lock(&self, webview_state: &mut WebviewState) {
        webview_state.evaluate_script("window.__wry_acquire_handler_lock()");
    }

    fn immediately_handle_ipc_message(
        &self,
        webview_state: &mut WebviewState,
        ipc_msg: IPCMessage,
    ) {
        webview_state.messages.receive_rust_message(ipc_msg);
    }
}

/// RAII guard for a held JS lock.
///
/// Holding the guard means a JS XHR is parked, suspending the JS event loop so
/// Rust can drive JS. Dropping it replies to that XHR, handing control back to
/// the JS event loop until the next wake. Tying release to the guard's scope
/// keeps acquire and release paired.
struct JsLockGuard {
    proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    webview_id: u64,
}

impl JsLockGuard {
    fn acquire(ipc: &WryIPC) -> Self {
        Self {
            proxy: ipc.proxy.clone(),
            webview_id: ipc.webview_id(),
        }
||||||| /tmp/base_wry.rs
    fn immediately_handle_ipc_message(
        &self,
        webview_state: &mut WebviewState,
        ipc_msg: OutboundIPCMessage,
    ) {
        let Some(message) = webview_state.messages.receive_rust_message(ipc_msg) else {
            return;
        };
        let decoded = message.decoded().unwrap();
        if let DecodedVariant::Evaluate { .. } = decoded {
            // Encode the binary data as base64 and pass to JS
            // JS will iterate over operations in the buffer
            let engine = base64::engine::general_purpose::STANDARD;
            let data_base64 = engine.encode(message.data());
            let code = format!("window.evaluate_from_rust_binary(\"{data_base64}\")");
            webview_state.evaluate_script(&code);
        }
=======
impl Default for WryBindgen {
    fn default() -> Self {
        Self::new()
>>>>>>> /tmp/theirs_wry.rs
    }
}

<<<<<<< /tmp/ours_wry.rs
impl Drop for JsLockGuard {
    fn drop(&mut self) {
        (self.proxy)(WryBindgenEvent::release_lock(self.webview_id));
    }
}

/// A builder for the application future and protocol handler.
pub struct AppBuilder<'a> {
    bindgen: &'a WryBindgen,
    ipc: WryIPC,
||||||| /tmp/base_wry.rs
/// A builder for the application future and protocol handler.
pub struct AppBuilder<'a> {
    webview_id: u64,
    bindgen: &'a WryBindgen,
    ipc: WryIPC,
=======
/// RAII guard for a held JS lock.
///
/// Holding the guard means a JS XHR is parked, suspending the JS event loop so
/// Rust can drive JS. Dropping it replies to that XHR, handing control back to
/// the JS event loop until the next wake. Tying release to the guard's scope
/// keeps acquire and release paired.
struct JsLockGuard {
    commands: DriverCommandSender,
>>>>>>> /tmp/theirs_wry.rs
}

<<<<<<< /tmp/ours_wry.rs
impl<'a> AppBuilder<'a> {
    /// Get the protocol handler for this webview.
    pub fn protocol_handler(&self) -> ProtocolHandler {
        ProtocolHandler {
            id: self.ipc.webview_id(),
            webview: self.bindgen.webview.clone(),
||||||| /tmp/base_wry.rs
impl<'a> AppBuilder<'a> {
    /// Get the protocol handler for this webview.
    pub fn protocol_handler(&self) -> ProtocolHandler {
        ProtocolHandler {
            id: self.webview_id,
            webview: self.bindgen.webview.clone(),
=======
impl JsLockGuard {
    fn acquire(ipc: &WryIPC) -> Self {
        Self {
            commands: ipc.command_sender(),
>>>>>>> /tmp/theirs_wry.rs
        }
    }
}

impl Drop for JsLockGuard {
    fn drop(&mut self) {
        self.commands.send(DriverCommand::ReleaseLock);
    }
}

/// Runtime endpoint moved to the app thread.
pub struct WryBindgenRuntime {
    ipc: WryIPC,
}

<<<<<<< /tmp/ours_wry.rs
    /// Consume the builder and get the prepared app future.
    pub fn build<F, F2, F3>(
||||||| /tmp/base_wry.rs
    /// Consume the builder and get the prepared app future.
    pub fn build<F>(
=======
impl WryBindgenRuntime {
    /// Build a sendable wrapper that creates and runs the app future on the
    /// thread where it is awaited.
    pub fn run<F, Fut>(
>>>>>>> /tmp/theirs_wry.rs
        self,
<<<<<<< /tmp/ours_wry.rs
        app: F,
        evaluate_script: F2,
    ) -> impl IntoFuture<Output = (), IntoFuture: 'static> + Send + 'static
||||||| /tmp/base_wry.rs
        app: impl FnOnce() -> F + Send + 'static,
        evaluate_script: impl FnMut(&str) + 'static,
    ) -> PreparedApp
=======
        app: F,
    ) -> impl IntoFuture<Output = (), IntoFuture: 'static> + Send + 'static
>>>>>>> /tmp/theirs_wry.rs
    where
<<<<<<< /tmp/ours_wry.rs
        F: FnOnce() -> F3 + Send + 'static,
        F2: FnMut(&str) + 'static,
        F3: core::future::Future<Output = ()> + 'static,
||||||| /tmp/base_wry.rs
        F: core::future::Future<Output = ()> + 'static,
=======
        F: FnOnce() -> Fut + Send + 'static,
        Fut: core::future::Future<Output = ()> + 'static,
>>>>>>> /tmp/theirs_wry.rs
    {
        struct RuntimeFuture<F, Fut> {
            app: F,
            ipc: WryIPC,
            phantom: core::marker::PhantomData<fn(Fut)>,
        }

        impl<F, Fut> RuntimeFuture<F, Fut> {
            fn new(app: F, ipc: WryIPC) -> Self {
                Self {
                    app,
                    ipc,
                    phantom: core::marker::PhantomData,
                }
            }
        }

        impl<F, Fut> IntoFuture for RuntimeFuture<F, Fut>
        where
            F: FnOnce() -> Fut + Send + 'static,
            Fut: core::future::Future<Output = ()> + 'static,
        {
<<<<<<< /tmp/ours_wry.rs
            let mut webviews = self.bindgen.webview.borrow_mut();
            let webview_state = webviews
                .get_mut(&self.ipc.webview_id())
                .expect("The webview state was created in WryBindgen::spawner");
            webview_state.evaluate_script = Box::new(evaluate_script);
||||||| /tmp/base_wry.rs
            let mut webviews = self.bindgen.webview.borrow_mut();
            let webview_state = webviews
                .get_mut(&self.webview_id)
                .expect("The webview state was created in WryBindgen::spawner");
            webview_state.evaluate_script = Box::new(evaluate_script);
=======
            type IntoFuture = Pin<Box<dyn core::future::Future<Output = ()>>>;
            type Output = ();

            fn into_future(self) -> Self::IntoFuture {
                let Self { app, ipc, .. } = self;
                let mut runtime = Some(Runtime::new(ipc));
                let mut app = Some(app);
                let mut run_app = None::<Pin<Box<Fut>>>;

                // The runtime drives the JS event loop by parking a synchronous XHR
                // (the "lock"). On each wake we drain inbound items from the shared
                // channel; `just_polled_app` distinguishes "the app future just
                // parked itself, stay idle" from "a wake means the app future wants
                // to run, so ask JS to park an XHR for the next poll".
                let poll_driver = poll_fn(move |ctx| {
                    let mut just_polled_app = false;
                    loop {
                        let Some(rt) = runtime.as_ref() else {
                            return Poll::Ready(());
                        };
                        match rt.ipc().poll_recv(ctx) {
                            // An idle JS→Rust callback. It replies through its own
                            // parked XHR, so we just dispatch it; the app future may
                            // now want polling, which the next Pending below requests.
                            Poll::Ready(Some(Inbound::Message(msg))) => {
                                let owned = runtime.take().expect("runtime available");
                                let (owned, _) =
                                    in_runtime(owned, || dispatch_inbound_message(&msg));
                                runtime = Some(owned);
                                just_polled_app = false;
                            }
                            // A parked XHR is available: poll the app future while the
                            // guard holds the lock, releasing it when the poll returns.
                            Poll::Ready(Some(Inbound::LockReady)) => {
                                let _guard = JsLockGuard::acquire(rt.ipc());
                                if run_app.is_none() {
                                    run_app = Some(Box::pin(app
                                        .take()
                                        .expect("app constructor called once")(
                                    )));
                                }
                                let owned = runtime.take().expect("runtime available");
                                let (owned, poll_result) = in_runtime(owned, || {
                                    run_app
                                        .as_mut()
                                        .expect("app future must exist")
                                        .as_mut()
                                        .poll(ctx)
                                });
                                runtime = Some(owned);
                                if poll_result.is_ready() {
                                    return Poll::Ready(());
                                }
                                just_polled_app = true;
                            }
                            Poll::Ready(None) => return Poll::Ready(()),
                            Poll::Pending => {
                                // Woken with no parked XHR. Unless we just polled the
                                // app future (it registered its own waker and is idle),
                                // this wake means it wants to run: ask JS to park an XHR.
                                if !just_polled_app {
                                    rt.ipc().send_acquire_lock();
                                }
                                return Poll::Pending;
                            }
                        }
                    }
                });

                Box::pin(poll_driver)
            }
>>>>>>> /tmp/theirs_wry.rs
        }

<<<<<<< /tmp/ours_wry.rs
        struct BuildFuture<F, F2> {
            app: F,
            ipc: WryIPC,
            phantom: core::marker::PhantomData<fn(F2)>,
        }
||||||| /tmp/base_wry.rs
        let start_future = move || {
            let run_app_in_runtime = async move {
                let run_app = app();
                let wait_for_events = handle_callbacks();
=======
        RuntimeFuture::new(app, self.ipc)
    }
}
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
        impl<F, F2> BuildFuture<F, F2> {
            fn new(app: F, ipc: WryIPC) -> Self {
                Self {
                    app,
                    ipc,
                    phantom: core::marker::PhantomData,
                }
            }
        }
||||||| /tmp/base_wry.rs
                futures_util::select! {
                    _ = run_app.fuse() => {},
                    _ = wait_for_events.fuse() => {},
                }
            };

            let runtime = Runtime::new(self.ipc, self.webview_id);
            let mut maybe_runtime = Some(runtime);
            let poll_in_runtime = async move {
                let mut run_app_in_runtime = pin!(run_app_in_runtime);
                poll_fn(move |ctx| {
                    let (new_runtime, poll_result) =
                        in_runtime(maybe_runtime.take().unwrap(), || {
                            run_app_in_runtime.as_mut().poll(ctx)
                        });
                    maybe_runtime = Some(new_runtime);
                    poll_result
                })
                .await
            };
=======
/// Main-thread driver for one webview session.
pub struct WryBindgenDriver {
    webview: Rc<RefCell<WebviewState>>,
    commands: DriverCommandReceiver,
}

impl WryBindgenDriver {
    /// Attach the driver to the webview's script evaluator.
    ///
    /// The evaluator is only used to ask JavaScript to acquire the synchronous
    /// handler lock before polling the async runtime.
    pub fn with_evaluate_script(
        self,
        evaluate_script: impl FnMut(&str) + 'static,
    ) -> WryBindgenWebviewDriver {
        WryBindgenWebviewDriver {
            driver: self,
            evaluate_script: Box::new(evaluate_script),
        }
    }
}
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
        impl<F, F2> IntoFuture for BuildFuture<F, F2>
        where
            F: FnOnce() -> F2 + Send + 'static,
            F2: core::future::Future<Output = ()> + 'static,
        {
            type IntoFuture = Pin<Box<dyn core::future::Future<Output = ()>>>;
            type Output = ();

            fn into_future(self) -> Self::IntoFuture {
                let Self { app, ipc, .. } = self;
                let mut runtime = Some(Runtime::new(ipc));
                let mut app = Some(app);
                let mut run_app = None::<Pin<Box<F2>>>;

                // The runtime drives the JS event loop by parking a synchronous XHR
                // (the "lock"). On each wake we drain inbound items from the shared
                // channel; `just_polled_app` distinguishes "the app future just
                // parked itself, stay idle" from "a wake means the app future wants
                // to run, so ask JS to park an XHR for the next poll".
                let poll_driver = poll_fn(move |ctx| {
                    let mut just_polled_app = false;
                    loop {
                        let Some(rt) = runtime.as_ref() else {
                            return Poll::Ready(());
                        };
                        match rt.ipc().poll_recv(ctx) {
                            // An idle JS→Rust callback. It replies through its own
                            // parked XHR, so we just dispatch it; the app future may
                            // now want polling, which the next Pending below requests.
                            Poll::Ready(Some(Inbound::Message(msg))) => {
                                let owned = runtime.take().expect("runtime available");
                                let (owned, _) =
                                    in_runtime(owned, || dispatch_inbound_message(&msg));
                                runtime = Some(owned);
                                just_polled_app = false;
                            }
                            // A parked XHR is available: poll the app future while the
                            // guard holds the lock, releasing it when the poll returns.
                            Poll::Ready(Some(Inbound::LockReady)) => {
                                let _guard = JsLockGuard::acquire(rt.ipc());
                                if run_app.is_none() {
                                    run_app = Some(Box::pin(app
                                        .take()
                                        .expect("app constructor called once")(
                                    )));
                                }
                                let owned = runtime.take().expect("runtime available");
                                let (owned, poll_result) = in_runtime(owned, || {
                                    run_app
                                        .as_mut()
                                        .expect("app future must exist")
                                        .as_mut()
                                        .poll(ctx)
                                });
                                runtime = Some(owned);
                                if poll_result.is_ready() {
                                    return Poll::Ready(());
                                }
                                just_polled_app = true;
                            }
                            Poll::Ready(None) => return Poll::Ready(()),
                            Poll::Pending => {
                                // Woken with no parked XHR. Unless we just polled the
                                // app future (it registered its own waker and is idle),
                                // this wake means it wants to run: ask JS to park an XHR.
                                if !just_polled_app {
                                    rt.ipc().send_acquire_lock();
                                }
                                return Poll::Pending;
                            }
                        }
                    }
                });
||||||| /tmp/base_wry.rs
            Box::pin(poll_in_runtime) as Pin<Box<dyn Future<Output = ()> + 'static>>
        };
=======
/// Main-thread driver bound to the webview's script evaluator.
pub struct WryBindgenWebviewDriver {
    driver: WryBindgenDriver,
    evaluate_script: Box<dyn FnMut(&str)>,
}
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
                Box::pin(poll_driver)
            }
||||||| /tmp/base_wry.rs
        PreparedApp {
            id: self.webview_id,
            future: Box::new(start_future),
=======
impl WryBindgenWebviewDriver {
    /// Poll the main-thread driver and evaluate scripts only when acquiring the
    /// JS lock for an async runtime poll.
    pub fn poll(&mut self, cx: &mut core::task::Context<'_>) -> Poll<()> {
        loop {
            match self.driver.commands.poll_recv(cx) {
                Poll::Ready(Some(command)) => {
                    let action = self
                        .driver
                        .webview
                        .borrow_mut()
                        .handle_driver_command(command);
                    action.run(&mut self.evaluate_script);
                }
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Pending => return Poll::Pending,
            }
>>>>>>> /tmp/theirs_wry.rs
        }

        BuildFuture::new(app, self.ipc)
    }
}

/// Create a blank HTTP response.
<<<<<<< /tmp/ours_wry.rs
pub(crate) fn blank_response() -> http::Response<Vec<u8>> {
||||||| /tmp/base_wry.rs
pub fn blank_response() -> http::Response<Vec<u8>> {
=======
fn blank_response() -> http::Response<Vec<u8>> {
>>>>>>> /tmp/theirs_wry.rs
    http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

/// Create an error HTTP response.
<<<<<<< /tmp/ours_wry.rs
pub(crate) fn error_response() -> http::Response<Vec<u8>> {
||||||| /tmp/base_wry.rs
pub fn error_response() -> http::Response<Vec<u8>> {
=======
fn error_response() -> http::Response<Vec<u8>> {
>>>>>>> /tmp/theirs_wry.rs
    http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

/// Create a JavaScript module HTTP response.
<<<<<<< /tmp/ours_wry.rs
pub(crate) fn module_response(content: &str) -> http::Response<Vec<u8>> {
||||||| /tmp/base_wry.rs
pub fn module_response(content: &str) -> http::Response<Vec<u8>> {
=======
fn module_response(content: &str) -> http::Response<Vec<u8>> {
>>>>>>> /tmp/theirs_wry.rs
    http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .header("access-control-allow-origin", "*")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

/// Create a not found HTTP response.
<<<<<<< /tmp/ours_wry.rs
pub(crate) fn not_found_response() -> http::Response<Vec<u8>> {
||||||| /tmp/base_wry.rs
pub fn not_found_response() -> http::Response<Vec<u8>> {
=======
fn not_found_response() -> http::Response<Vec<u8>> {
>>>>>>> /tmp/theirs_wry.rs
    http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}

#[cfg(test)]
mod tests {
    use super::*;
<<<<<<< /tmp/ours_wry.rs
    use crate::ipc::{DecodedVariant, MessageType};
||||||| /tmp/base_wry.rs
    use crate::ipc::EncodedData;
=======
    use crate::ipc::{DecodedVariant, EncodedData, MessageType};
    use std::sync::Arc;
>>>>>>> /tmp/theirs_wry.rs

    fn ipc_message(message_type: MessageType) -> IPCMessage {
<<<<<<< /tmp/ours_wry.rs
        crate::ipc::empty_message(message_type)
||||||| /tmp/base_wry.rs
        let mut data = EncodedData::new();
        data.push_u8(message_type as u8);
        IPCMessage::new(data.to_bytes())
=======
        let mut data = EncodedData::default();
        data.push_u8(message_type as u8);
        IPCMessage::new(data.to_bytes())
>>>>>>> /tmp/theirs_wry.rs
    }

    fn handler_request(message_type: MessageType) -> http::Request<Vec<u8>> {
        let engine = base64::engine::general_purpose::STANDARD;
        let body_base64 = engine.encode(ipc_message(message_type).data());

        http::Request::builder()
            .uri("wry://index.html/__wbg__/handler")
            .header("dioxus-data", body_base64)
            .body(Vec::new())
            .expect("failed to build request")
    }

<<<<<<< /tmp/ours_wry.rs
    fn lock_request() -> http::Request<Vec<u8>> {
        http::Request::builder()
            .uri("wry://index.html/__wbg__/handler")
            .header("wry-bindgen-lock", "1")
            .body(Vec::new())
            .expect("failed to build request")
    }

    struct NoopWake;

    impl std::task::Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn poll_forwarded_message(ipc: &WryIPC) -> IPCMessage {
        let waker = std::task::Waker::from(Arc::new(NoopWake));
        let mut cx = std::task::Context::from_waker(&waker);
        match ipc.poll_recv(&mut cx) {
            Poll::Ready(Some(Inbound::Message(msg))) => msg,
            other => panic!("expected forwarded IPC message, got {other:?}"),
        }
    }

    #[test]
    fn js_respond_is_forwarded_and_parks_xhr() {
        let (ipc, sender) = WryIPC::new(Arc::new(|_| {}), 0);
        let mut layer = WebviewMessageLayer::new(sender);
        let responder_called = Rc::new(RefCell::new(false));
        let captured_responder_called = responder_called.clone();

        layer.receive_js_message(
            ipc_message(MessageType::Respond),
            WryBindgenResponder::from(move |_| {
                *captured_responder_called.borrow_mut() = true;
            }),
        );

        assert!(layer.current_xhr.is_some());
        assert!(
            !*responder_called.borrow(),
            "JS response XHR should stay parked for Rust's next reply"
        );
        let received = poll_forwarded_message(&ipc);
        assert!(matches!(
            received.decoded().unwrap(),
            DecodedVariant::Respond { .. }
        ));
    }

    #[test]
    fn js_message_while_xhr_is_parked_panics() {
        let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}), 0);
        let mut layer = WebviewMessageLayer::new(sender);
        layer.current_xhr = Some(WryBindgenResponder::from(|_| {}));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            layer.receive_js_message(
                ipc_message(MessageType::Evaluate),
                WryBindgenResponder::from(|_| {}),
            );
        }));

        assert!(result.is_err());
    }

    #[test]
    fn lock_request_while_xhr_is_parked_panics() {
        let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}), 0);
        let mut layer = WebviewMessageLayer::new(sender);
        layer.current_xhr = Some(WryBindgenResponder::from(|_| {}));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            layer.receive_lock_request(WryBindgenResponder::from(|_| {}));
        }));

        assert!(result.is_err());
    }

    #[test]
    fn rust_outbound_messages_use_same_parked_xhr_response_path() {
        for message_type in [MessageType::Evaluate, MessageType::Respond] {
            let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}), 0);
            let mut layer = WebviewMessageLayer::new(sender);
            let response = Rc::new(RefCell::new(None));
            let captured_response = response.clone();
            let message = ipc_message(message_type);
            let expected_body = message.data().to_vec();

            layer.current_xhr = Some(WryBindgenResponder::from(move |response| {
                *captured_response.borrow_mut() = Some(response);
            }));
            layer.receive_rust_message(message);

            assert!(layer.current_xhr.is_none());
            let response = response
                .borrow_mut()
                .take()
                .expect("parked XHR should receive Rust IPC");
            assert_eq!(response.status(), http::StatusCode::OK);
            let engine = base64::engine::general_purpose::STANDARD;
            let body = engine
                .decode(response.body())
                .expect("response body should be base64 IPC bytes");
            assert_eq!(body, expected_body);
        }
    }

||||||| /tmp/base_wry.rs
=======
    fn lock_request() -> http::Request<Vec<u8>> {
        http::Request::builder()
            .uri("wry://index.html/__wbg__/handler")
            .header("wry-bindgen-lock", "1")
            .body(Vec::new())
            .expect("failed to build request")
    }

    fn initialized_request() -> http::Request<Vec<u8>> {
        http::Request::builder()
            .uri("wry://index.html/__wbg__/initialized")
            .body(Vec::new())
            .expect("failed to build request")
    }

    struct NoopWake;

    impl std::task::Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn poll_forwarded_message(ipc: &WryIPC) -> IPCMessage {
        let waker = std::task::Waker::from(Arc::new(NoopWake));
        let mut cx = std::task::Context::from_waker(&waker);
        match ipc.poll_recv(&mut cx) {
            Poll::Ready(Some(Inbound::Message(msg))) => msg,
            other => panic!("expected forwarded IPC message, got {other:?}"),
        }
    }

    fn poll_driver(driver: &mut WryBindgenWebviewDriver) -> Poll<()> {
        let waker = std::task::Waker::from(Arc::new(NoopWake));
        let mut cx = std::task::Context::from_waker(&waker);
        driver.poll(&mut cx)
    }

    #[test]
    fn js_respond_is_forwarded_and_parks_xhr() {
        let (ipc, sender, _driver_commands) = WryIPC::new();
        let mut layer = WebviewMessageLayer::new(sender);
        let responder_called = Rc::new(RefCell::new(false));
        let captured_responder_called = responder_called.clone();

        layer.receive_js_message(
            ipc_message(MessageType::Respond),
            WryBindgenResponder::from(move |_| {
                *captured_responder_called.borrow_mut() = true;
            }),
        );

        assert!(layer.current_xhr.is_some());
        assert!(
            !*responder_called.borrow(),
            "JS response XHR should stay parked for Rust's next reply"
        );
        let received = poll_forwarded_message(&ipc);
        assert!(matches!(
            received.decoded().unwrap(),
            DecodedVariant::Respond { .. }
        ));
    }

    #[test]
    fn js_message_while_xhr_is_parked_panics() {
        let (_ipc, sender, _driver_commands) = WryIPC::new();
        let mut layer = WebviewMessageLayer::new(sender);
        layer.current_xhr = Some(WryBindgenResponder::from(|_| {}));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            layer.receive_js_message(
                ipc_message(MessageType::Evaluate),
                WryBindgenResponder::from(|_| {}),
            );
        }));

        assert!(result.is_err());
    }

    #[test]
    fn lock_request_while_xhr_is_parked_panics() {
        let (_ipc, sender, _driver_commands) = WryIPC::new();
        let mut layer = WebviewMessageLayer::new(sender);
        layer.current_xhr = Some(WryBindgenResponder::from(|_| {}));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            layer.receive_lock_request(WryBindgenResponder::from(|_| {}));
        }));

        assert!(result.is_err());
    }

    #[test]
    fn rust_outbound_messages_use_same_parked_xhr_response_path() {
        for message_type in [MessageType::Evaluate, MessageType::Respond] {
            let (_ipc, sender, _driver_commands) = WryIPC::new();
            let mut layer = WebviewMessageLayer::new(sender);
            let response = Rc::new(RefCell::new(None));
            let captured_response = response.clone();
            let message = ipc_message(message_type);
            let expected_body = message.data().to_vec();

            layer.current_xhr = Some(WryBindgenResponder::from(move |response| {
                *captured_response.borrow_mut() = Some(response);
            }));
            layer.receive_rust_message(OutboundIPCMessage::new(message));

            assert!(layer.current_xhr.is_none());
            let response = response
                .borrow_mut()
                .take()
                .expect("parked XHR should receive Rust IPC");
            assert_eq!(response.status(), http::StatusCode::OK);
            let engine = base64::engine::general_purpose::STANDARD;
            let body = engine
                .decode(response.body())
                .expect("response body should be base64 IPC bytes");
            assert_eq!(body, expected_body);
        }
    }

>>>>>>> /tmp/theirs_wry.rs
    #[test]
    fn handler_responds_error_when_evaluate_arrives_after_runtime_drop() {
        let bindgen = WryBindgen::new();
        let protocol_handler = bindgen.protocol_handler();
        drop(bindgen);

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = handler_request(MessageType::Evaluate);

        let unhandled = protocol_handler.handle_request("wry", &request, move |response| {
            *captured_response.borrow_mut() = Some(response)
        });

        assert!(unhandled.is_none());
        let response = response
            .borrow_mut()
            .take()
            .expect("closed runtime should receive an error response");
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
<<<<<<< /tmp/ours_wry.rs
    fn lock_request_is_queued_until_webview_loads() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let webview_id = app_builder.ipc.webview_id();

        let evaluated_scripts = Rc::new(RefCell::new(Vec::new()));
        let captured_scripts = evaluated_scripts.clone();
        let _prepared_app = app_builder.build(
            || async {},
            move |script| captured_scripts.borrow_mut().push(script.to_string()),
        );

        bindgen.handle_user_event(WryBindgenEvent::acquire_lock(webview_id));
        assert!(evaluated_scripts.borrow().is_empty());

        bindgen.handle_user_event(WryBindgenEvent::webview_loaded(webview_id));
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"]
        );
    }

    #[test]
    fn lock_request_while_js_xhr_is_parked_is_not_dropped_or_duplicated() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let webview_id = app_builder.ipc.webview_id();
        let protocol_handler = app_builder.protocol_handler();
||||||| /tmp/base_wry.rs
    fn handler_responds_error_when_top_level_respond_arrives_after_runtime_drop() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let webview_id = app_builder.webview_id;
        let protocol_handler = app_builder.protocol_handler();
=======
    fn lock_request_is_queued_until_webview_loads() {
        let bindgen = WryBindgen::new();
        let protocol_handler = bindgen.protocol_handler();
>>>>>>> /tmp/theirs_wry.rs

        let evaluated_scripts = Rc::new(RefCell::new(Vec::new()));
        let captured_scripts = evaluated_scripts.clone();
<<<<<<< /tmp/ours_wry.rs
        let _prepared_app = app_builder.build(
            || async {},
            move |script| captured_scripts.borrow_mut().push(script.to_string()),
||||||| /tmp/base_wry.rs
        let prepared_app = app_builder.build(
            || async {},
            move |script| captured_scripts.borrow_mut().push(script.to_string()),
=======
        let (runtime, driver) = bindgen.split();
        let mut driver = driver.with_evaluate_script(move |script| {
            captured_scripts.borrow_mut().push(script.to_string());
        });

        runtime.ipc.send_acquire_lock();
        assert!(matches!(poll_driver(&mut driver), Poll::Pending));
        assert!(evaluated_scripts.borrow().is_empty());

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = initialized_request();
        let unhandled = protocol_handler.handle_request("wry", &request, move |response| {
            *captured_response.borrow_mut() = Some(response)
        });

        assert!(unhandled.is_none());
        assert_eq!(
            response.borrow().as_ref().unwrap().status(),
            http::StatusCode::OK
        );
        assert!(matches!(poll_driver(&mut driver), Poll::Pending));
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"]
>>>>>>> /tmp/theirs_wry.rs
        );
    }

<<<<<<< /tmp/ours_wry.rs
        bindgen.handle_user_event(WryBindgenEvent::webview_loaded(webview_id));

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = handler_request(MessageType::Evaluate);

        let unhandled = protocol_handler.handle_request(
            "wry",
            |_| {},
            &request,
            move |response| *captured_response.borrow_mut() = Some(response),
        );

        assert!(unhandled.is_none());
        assert!(
            response.borrow().is_none(),
            "JS callback XHR should stay parked while Rust handles it"
        );

        bindgen.handle_user_event(WryBindgenEvent::acquire_lock(webview_id));
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"],
            "lock script should be requested while the parked XHR is outstanding"
        );

        bindgen.handle_user_event(WryBindgenEvent::ipc(
            webview_id,
            ipc_message(MessageType::Respond),
        ));
||||||| /tmp/base_wry.rs
        bindgen.handle_user_event(WryBindgenEvent::webview_loaded(webview_id));
        bindgen.handle_user_event(WryBindgenEvent::ipc(
            webview_id,
            OutboundIPCMessage::new(ipc_message(MessageType::Evaluate), true),
        ));
        assert_eq!(evaluated_scripts.borrow().len(), 1);
=======
    #[test]
    fn lock_request_while_js_xhr_is_parked_is_not_dropped_or_duplicated() {
        let bindgen = WryBindgen::new();
        let protocol_handler = bindgen.protocol_handler();

        let evaluated_scripts = Rc::new(RefCell::new(Vec::new()));
        let captured_scripts = evaluated_scripts.clone();
        let (runtime, driver) = bindgen.split();
        let mut driver = driver.with_evaluate_script(move |script| {
            captured_scripts.borrow_mut().push(script.to_string());
        });
>>>>>>> /tmp/theirs_wry.rs

<<<<<<< /tmp/ours_wry.rs
        let response = response
            .borrow_mut()
            .take()
            .expect("parked JS callback XHR should receive Rust's response");
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"],
            "answering the parked XHR should not duplicate the in-flight lock request"
        );
    }

    #[test]
    fn handler_responds_error_when_lock_arrives_after_runtime_drop() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let protocol_handler = app_builder.protocol_handler();
        drop(app_builder);
||||||| /tmp/base_wry.rs
        drop(prepared_app);
=======
        let request = initialized_request();
        let unhandled = protocol_handler.handle_request("wry", &request, |_| {});
        assert!(unhandled.is_none());
>>>>>>> /tmp/theirs_wry.rs

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
<<<<<<< /tmp/ours_wry.rs
        let request = lock_request();
||||||| /tmp/base_wry.rs
        let request = handler_request(MessageType::Respond);
=======
        let request = handler_request(MessageType::Evaluate);

        let unhandled = protocol_handler.handle_request("wry", &request, move |response| {
            *captured_response.borrow_mut() = Some(response)
        });

        assert!(unhandled.is_none());
        assert!(
            response.borrow().is_none(),
            "JS callback XHR should stay parked while Rust handles it"
        );
>>>>>>> /tmp/theirs_wry.rs

        runtime.ipc.send_acquire_lock();
        assert!(matches!(poll_driver(&mut driver), Poll::Pending));
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"],
            "lock script should be requested while the parked XHR is outstanding"
        );

        runtime
            .ipc
            .send_ipc(OutboundIPCMessage::new(ipc_message(MessageType::Respond)));
        assert!(matches!(poll_driver(&mut driver), Poll::Pending));

        let response = response
            .borrow_mut()
            .take()
            .expect("parked JS callback XHR should receive Rust's response");
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"],
            "answering the parked XHR should not duplicate the in-flight lock request"
        );
    }

    #[test]
    fn handler_responds_error_when_lock_arrives_after_runtime_drop() {
        let bindgen = WryBindgen::new();
        let protocol_handler = bindgen.protocol_handler();
        drop(bindgen);

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = lock_request();

        let unhandled = protocol_handler.handle_request("wry", &request, move |response| {
            *captured_response.borrow_mut() = Some(response)
        });

        assert!(unhandled.is_none());
        let response = response
            .borrow_mut()
            .take()
            .expect("closed runtime should receive an error response");
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }
}
