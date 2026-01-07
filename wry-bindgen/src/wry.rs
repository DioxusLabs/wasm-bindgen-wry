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

use http::Response;

use crate::function_registry::FUNCTION_REGISTRY;
use crate::ipc::{DecodedVariant, IPCMessage, MessageType, decode_data};
use crate::runtime::{AppEvent, AppEventVariant, get_runtime};

/// Responder for wry-bindgen protocol requests.
pub struct WryBindgenResponder {
    respond: Box<dyn FnOnce(Response<Vec<u8>>)>,
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
#[derive(Default)]
struct SharedWebviewState {
    ongoing_request: Option<WryBindgenResponder>,
    /// How many responses we are waiting for from JS
    pending_js_evaluates: usize,
    /// How many responses JS is waiting for from us
    pending_rust_evaluates: usize,
}

impl SharedWebviewState {
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
            responder.respond(
                http::Response::builder()
                    .status(200)
                    .header("Content-Type", "application/octet-stream")
                    .body(body)
                    .expect("Failed to build response"),
            );
        } else {
            panic!("WARNING: respond_to_request called but no pending request! Response dropped.");
        }
    }
}

/// Reusable wry-bindgen state for integrating with existing wry applications.
///
/// This struct manages the IPC protocol between Rust and JavaScript,
/// handling message queuing, async responses, and JS function registration.
///
/// # Example
///
/// ```ignore
/// let wry_bindgen = WryBindgen::new(&FUNCTION_REGISTRY);
///
/// let protocol_handler = wry_bindgen.create_protocol_handler(
///     move |event| { proxy.send_event(event).ok(); },
///     || my_custom_root_html(),
/// );
///
/// let webview = WebViewBuilder::new()
///     .with_asynchronous_custom_protocol("wry".into(), move |_, req, resp| {
///         protocol_handler(&req, resp);
///     })
///     .with_url("wry://index")
///     .build(&window)?;
///
/// webview.evaluate_script(wry_bindgen.init_script())?;
/// ```
pub struct WryBindgen {
    // State that is shared with the protocol handler closure
    shared: Rc<RefCell<SharedWebviewState>>,
    // The state of the webview. Either loading (with queued messages) or loaded.
    state: RefCell<WebviewLoadingState>,
}

impl Default for WryBindgen {
    fn default() -> Self {
        Self::new()
    }
}

impl WryBindgen {
    /// Create a new WryBindgen instance.
    ///
    /// # Arguments
    /// * `function_registry` - Reference to the collected JS function specifications
    pub fn new() -> Self {
        Self {
            shared: Rc::new(RefCell::new(SharedWebviewState::default())),
            state: RefCell::new(WebviewLoadingState::default()),
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

    /// Create a protocol handler closure suitable for `WebViewBuilder::with_asynchronous_custom_protocol`.
    ///
    /// The returned closure handles all "wry://" protocol requests:
    /// - "wry://index" - serves root HTML (uses provided root_response)
    /// - "wry://ready" - signals webview loaded
    /// - "wry://snippets/{path}" - serves inline JS modules
    /// - "wry://handler" - main IPC endpoint
    ///
    /// # Arguments
    /// * `proxy` - Function to send events to the event loop
    /// * `root_response` - Function that returns the HTML response to serve at "wry://index"
    pub fn create_protocol_handler<F, R: Into<WryBindgenResponder>>(
        &self,
        proxy: F,
    ) -> impl Fn(&http::Request<Vec<u8>>, R) -> Option<R> + 'static
    where
        F: Fn(AppEvent) + 'static,
    {
        let shared = self.shared.clone();

        move |request: &http::Request<Vec<u8>>, responder: R| {
            let real_path = request.uri().to_string().replace("wry://", "");
            let real_path = real_path.as_str().trim_matches('/');

            if real_path == "init" {
                let responder = responder.into();
                responder.respond(module_response(&Self::init_script()));
                return None;
            }

            if real_path == "ready" {
                proxy(AppEvent::webview_loaded());
                let responder = responder.into();
                responder.respond(blank_response());
                return None;
            }

            // Serve inline_js modules from snippets/
            if real_path.starts_with("snippets/") {
                let responder = responder.into();
                if let Some(content) = FUNCTION_REGISTRY.get_module(real_path) {
                    responder.respond(module_response(content));
                    return None;
                }
                responder.respond(not_found_response());
                return None;
            }

            // Js sent us either an Evaluate or Respond message
            if real_path == "handler" {
                let responder = responder.into();
                let mut shared = shared.borrow_mut();
                let Some(msg) = decode_request_data(request) else {
                    responder.respond(error_response());
                    return None;
                };
                let msg_type = msg.ty().unwrap();
                match msg_type {
                    // New call from JS - save responder and wait for the js application thread to respond
                    MessageType::Evaluate => {
                        shared.pending_rust_evaluates += 1;
                        shared.set_ongoing_request(responder);
                    }
                    // Response from JS to a previous Evaluate - decrement pending count and respond accordingly
                    MessageType::Respond => {
                        shared.pending_js_evaluates = shared.pending_js_evaluates.saturating_sub(1);
                        if shared.pending_rust_evaluates > 0 || shared.pending_js_evaluates > 0 {
                            // Still more round-trips expected
                            shared.set_ongoing_request(responder);
                        } else {
                            // Conversation is over
                            responder.respond(blank_response());
                        }
                    }
                }
                get_runtime().queue_rust_call(msg);
                return None;
            }

            Some(responder)
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
    pub fn handle_user_event(
        &self,
        event: AppEvent,
        evaluate_script: impl FnOnce(&str),
    ) -> Option<i32> {
        match event.into_variant() {
            AppEventVariant::Shutdown(status) => {
                return Some(status);
            }
            AppEventVariant::RunOnMainThread(task) => {
                task.execute();
                return None;
            }
            // The rust thread sent us an IPCMessage to send to JS
            AppEventVariant::Ipc(ipc_msg) => {
                {
                    let mut state = self.state.borrow_mut();
                    if let WebviewLoadingState::Pending { queued } = &mut *state {
                        queued.push(ipc_msg);
                        return None;
                    }
                }

                let mut shared = self.shared.borrow_mut();

                let ty = ipc_msg.ty().unwrap();
                match ty {
                    // Rust wants to evaluate something in js
                    MessageType::Evaluate => {
                        shared.pending_js_evaluates += 1;
                    }
                    // Rust is responding to a previous js evaluate
                    MessageType::Respond => {
                        shared.pending_rust_evaluates =
                            shared.pending_rust_evaluates.saturating_sub(1);
                    }
                }

                // If there is an ongoing request, respond to immediately
                if shared.has_pending_request() {
                    shared.respond_to_request(ipc_msg);
                    return None;
                }

                // Otherwise call into js through evaluate_script
                let decoded = ipc_msg.decoded().unwrap();

                if let DecodedVariant::Evaluate { .. } = decoded {
                    // Encode the binary data as base64 and pass to JS
                    // JS will iterate over operations in the buffer
                    let engine = base64::engine::general_purpose::STANDARD;
                    let data_base64 = engine.encode(ipc_msg.data());
                    let code = format!("window.evaluate_from_rust_binary(\"{data_base64}\")");
                    evaluate_script(&code);
                }
            }
            AppEventVariant::WebviewLoaded => {
                let mut state = self.state.borrow_mut();
                if let WebviewLoadingState::Pending { queued } =
                    std::mem::replace(&mut *state, WebviewLoadingState::Loaded)
                {
                    for msg in queued {
                        get_runtime().js_response(msg);
                    }
                }
            }
        }
        None
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
