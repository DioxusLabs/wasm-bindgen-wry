//! Reusable wry-bindgen state for integrating with existing wry applications.
//!
//! This module provides [`WryBindgen`], a struct that manages the IPC protocol
//! between Rust and JavaScript. It can be injected into any wry application
//! to enable wry-bindgen functionality.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use base64::Engine;
use core::cell::RefCell;
use core::future::poll_fn;
use core::pin::{Pin, pin};
use futures_util::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;

use http::Response;

use crate::batch::{Runtime, in_runtime};
use crate::function_registry::FUNCTION_REGISTRY;
use crate::ipc::{DecodedVariant, IPCMessage, MessageType, decode_data};
use crate::runtime::{AppEventVariant, IPCSenders, WryBindgenEvent, WryIPC, handle_callbacks};

pub trait ImplWryBindgenResponder {
    fn respond(self: Box<Self>, response: Response<Vec<u8>>);
}

/// Responder for wry-bindgen protocol requests.
pub struct WryBindgenResponder {
    respond: Box<dyn ImplWryBindgenResponder>,
}

impl<F> From<F> for WryBindgenResponder
where
    F: FnOnce(Response<Vec<u8>>) + 'static,
{
    fn from(respond: F) -> Self {
        struct FnOnceWrapper<F> {
            f: F,
        }

        impl<F> ImplWryBindgenResponder for FnOnceWrapper<F>
        where
            F: FnOnce(Response<Vec<u8>>) + 'static,
        {
            fn respond(self: Box<Self>, response: Response<Vec<u8>>) {
                (self.f)(response)
            }
        }

        Self {
            respond: Box::new(FnOnceWrapper { f: respond }),
        }
    }
}

impl WryBindgenResponder {
    pub fn new(f: impl ImplWryBindgenResponder + 'static) -> Self {
        Self {
            respond: Box::new(f),
        }
    }

    fn respond(self, response: Response<Vec<u8>>) {
        self.respond.respond(response);
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
    /// Webview is still loading, messages are queued.
    Pending { queued: Vec<IPCMessage> },
    /// Webview is loaded and ready.
    Loaded,
}

impl Default for WebviewLoadingState {
    fn default() -> Self {
        WebviewLoadingState::Pending { queued: Vec::new() }
    }
}

/// Shared state for managing async protocol responses.
struct WebviewState {
    ongoing_request: Option<WryBindgenResponder>,
    /// How many responses we are waiting for from JS
    pending_js_evaluates: usize,
    /// How many responses JS is waiting for from us
    pending_rust_evaluates: usize,
    /// The sender used to send IPC messages to the webview
    sender: IPCSenders,
    // The state of the webview. Either loading (with queued messages) or loaded.
    loading_state: WebviewLoadingState,
    // A function that evaluates scripts in the webview
    evaluate_script: Box<dyn FnMut(&str)>,
}

impl WebviewState {
    /// Create a new webview state.
    fn new(sender: IPCSenders, evaluate_script: impl FnMut(&str) + 'static) -> Self {
        Self {
            ongoing_request: None,
            pending_js_evaluates: 0,
            pending_rust_evaluates: 0,
            sender,
            loading_state: WebviewLoadingState::default(),
            evaluate_script: Box::new(evaluate_script),
        }
    }

    fn set_ongoing_request(&mut self, responder: WryBindgenResponder) {
        if self.ongoing_request.is_some() {
            panic!(
                "WARNING: Overwriting existing ongoing_request! Previous request will never be responded to."
            );
        }
        self.ongoing_request = Some(responder);
    }

    fn take_ongoing_request(&mut self) -> Option<WryBindgenResponder> {
        self.ongoing_request.take()
    }

    fn has_pending_request(&self) -> bool {
        self.ongoing_request.is_some()
    }

    fn respond_to_request(&mut self, response: IPCMessage) {
        if let Some(responder) = self.take_ongoing_request() {
            let body = response.into_data();
            // Encode as base64 - sync XMLHttpRequest cannot use responseType="arraybuffer"
            let engine = base64::engine::general_purpose::STANDARD;
            let body_base64 = engine.encode(&body);
            responder.respond(
                http::Response::builder()
                    .status(200)
                    .header("Content-Type", "text/plain")
                    .body(body_base64.into_bytes())
                    .expect("Failed to build response"),
            );
        } else {
            panic!("WARNING: respond_to_request called but no pending request! Response dropped.");
        }
    }

    fn evaluate_script(&mut self, script: &str) {
        (self.evaluate_script)(script);
    }
}

fn unique_id() -> u64 {
    use core::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

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

impl PreparedApp {
    /// Get the unique id of this PreparedApp.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get the inner future of this PreparedApp.
    pub fn into_future(self) -> Pin<Box<dyn core::future::Future<Output = ()> + 'static>> {
        (self.future)()
    }
}

/// Factory for creating a protocol handler for a specific webview.
///
/// This struct is NOT `Send` because it holds a reference to shared webview state.
/// Create the protocol handler on the main thread before spawning the app thread.
pub struct ProtocolHandler {
    id: u64,
    webview: Rc<RefCell<HashMap<u64, WebviewState>>>,
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
    /// * `proxy` - Function to send events to the event loop
    pub fn handle_request<F, R: Into<WryBindgenResponder>>(
        &self,
        protocol: &str,
        proxy: F,
        request: &http::Request<Vec<u8>>,
        responder: R,
    ) -> Option<R>
    where
        F: Fn(WryBindgenEvent),
    {
        let webviews = &self.webview;
        let webview_id = self.id;

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
            let responder = responder.into();
            if let Some(content) = FUNCTION_REGISTRY.get_module(path_without_snippets) {
                responder.respond(module_response(content));
                return None;
            }
            responder.respond(not_found_response());
            return None;
        }

        if path_without_wbg == "init.js" {
            let responder = responder.into();
            responder.respond(module_response(&init_script()));
            return None;
        }

        if path_without_wbg == "initialized" {
            proxy(WryBindgenEvent::webview_loaded(webview_id));
            let responder = responder.into();
            responder.respond(blank_response());
            return None;
        }

        // Js sent us either an Evaluate or Respond message
        if path_without_wbg == "handler" {
            let responder = responder.into();
            let mut webviews = webviews.borrow_mut();
            let Some(webview_state) = webviews.get_mut(&webview_id) else {
                responder.respond(error_response());
                return None;
            };
            let Some(msg) = decode_request_data(request) else {
                responder.respond(error_response());
                return None;
            };
            let msg_type = msg.ty().unwrap();
            match msg_type {
                // New call from JS - save responder and wait for the js application thread to respond
                MessageType::Evaluate => {
                    webview_state.pending_rust_evaluates += 1;
                    webview_state.set_ongoing_request(responder);
                }
                // Response from JS to a previous Evaluate - decrement pending count and respond accordingly
                MessageType::Respond => {
                    webview_state.pending_js_evaluates =
                        webview_state.pending_js_evaluates.saturating_sub(1);
                    if webview_state.pending_rust_evaluates > 0
                        || webview_state.pending_js_evaluates > 0
                    {
                        // Still more round-trips expected
                        webview_state.set_ongoing_request(responder);
                    } else {
                        // Conversation is over
                        responder.respond(blank_response());
                    }
                }
            }
            webview_state.sender.start_send(msg);
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

/// Reusable wry-bindgen state for integrating with existing wry applications.
///
/// This struct manages the IPC protocol between Rust and JavaScript,
/// handling message queuing, async responses, and JS function registration.
///
/// # Example
///
/// ```ignore
/// let wry_bindgen = WryBindgen::new(move |event| { proxy.send_event(event).ok(); });
///
/// let (prepared_app, protocol_factory) = wry_bindgen.in_runtime(|| async { my_app().await });
/// let protocol_handler = protocol_factory.create("wry", move |event| {
///     proxy.send_event(event).ok();
/// });
///
/// std::thread::spawn(move || {
///     // Run prepared_app.into_future() in a tokio runtime
/// });
///
/// let webview = WebViewBuilder::new()
///     .with_asynchronous_custom_protocol("wry".into(), move |_, req, resp| {
///         protocol_handler(&req, resp);
///     })
///     .with_url("wry://index")
///     .build(&window)?;
/// ```
pub struct WryBindgen {
    event_loop_proxy: Arc<dyn Fn(WryBindgenEvent) + Send + Sync>,
    // State that is unique to each webview
    webview: Rc<RefCell<HashMap<u64, WebviewState>>>,
}

impl WryBindgen {
    /// Create a new WryBindgen instance.
    pub fn new(event_loop_proxy: impl Fn(WryBindgenEvent) + Send + Sync + 'static) -> Self {
        Self {
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
    }

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
        }
    }

    fn handle_ipc_message(&self, id: u64, ipc_msg: IPCMessage) {
        let mut state = self.webview.borrow_mut();
        let Some(webview_state) = state.get_mut(&id) else {
            return;
        };
        if let WebviewLoadingState::Pending { queued } = &mut webview_state.loading_state {
            queued.push(ipc_msg);
            return;
        }

        self.immediately_handle_ipc_message(webview_state, ipc_msg)
    }

    fn immediately_handle_ipc_message(
        &self,
        webview_state: &mut WebviewState,
        ipc_msg: IPCMessage,
    ) {
        let ty = ipc_msg.ty().unwrap();
        match ty {
            // Rust wants to evaluate something in js
            MessageType::Evaluate => {
                webview_state.pending_js_evaluates += 1;
            }
            // Rust is responding to a previous js evaluate
            MessageType::Respond => {
                webview_state.pending_rust_evaluates =
                    webview_state.pending_rust_evaluates.saturating_sub(1);
            }
        }

        // If there is an ongoing request, respond to immediately
        if webview_state.has_pending_request() {
            webview_state.respond_to_request(ipc_msg);
            return;
        }

        // Otherwise call into js through evaluate_script
        let decoded = ipc_msg.decoded().unwrap();

        if let DecodedVariant::Evaluate { .. } = decoded {
            // Encode the binary data as base64 and pass to JS
            // JS will iterate over operations in the buffer
            let engine = base64::engine::general_purpose::STANDARD;
            let data_base64 = engine.encode(ipc_msg.data());
            let code = format!("window.evaluate_from_rust_binary(\"{data_base64}\")");
            webview_state.evaluate_script(&code);
        }
    }
}

/// A builder for the application future and protocol handler.
pub struct AppBuilder<'a> {
    webview_id: u64,
    bindgen: &'a WryBindgen,
    ipc: WryIPC,
}

impl<'a> AppBuilder<'a> {
    /// Get the protocol handler for this webview.
    pub fn protocol_handler(&self) -> ProtocolHandler {
        ProtocolHandler {
            id: self.webview_id,
            webview: self.bindgen.webview.clone(),
        }
    }

    /// Consume the builder and get the prepared app future.
    pub fn build<F>(
        self,
        app: impl FnOnce() -> F + Send + 'static,
        evaluate_script: impl FnMut(&str) + 'static,
    ) -> PreparedApp
    where
        F: core::future::Future<Output = ()> + 'static,
    {
        // First set up the evaluate_script function in the webview state
        {
            let mut webviews = self.bindgen.webview.borrow_mut();
            let webview_state = webviews
                .get_mut(&self.webview_id)
                .expect("The webview state was created in WryBindgen::spawner");
            webview_state.evaluate_script = Box::new(evaluate_script);
        }

        let start_future = move || {
            let run_app_in_runtime = async move {
                let run_app = app();
                let wait_for_events = handle_callbacks();

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

            Box::pin(poll_in_runtime) as Pin<Box<dyn Future<Output = ()> + 'static>>
        };

        PreparedApp {
            id: self.webview_id,
            future: Box::new(start_future),
        }
    }
}

/// Create a blank HTTP response.
pub fn blank_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

/// Create an error HTTP response.
pub fn error_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

/// Create a JavaScript module HTTP response.
pub fn module_response(content: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .header("access-control-allow-origin", "*")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

/// Create a not found HTTP response.
pub fn not_found_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}
