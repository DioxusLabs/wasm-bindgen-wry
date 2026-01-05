//! Reusable wry-bindgen state for integrating with existing wry applications.
//!
//! This module provides [`WryBindgen`], a struct that manages the IPC protocol
//! between Rust and JavaScript. It can be injected into any wry application
//! to enable wry-bindgen functionality.

use base64::Engine;
use std::cell::RefCell;
use std::rc::Rc;

use wry::RequestAsyncResponder;

use wasm_bindgen::ipc::{decode_data, DecodedVariant, IPCMessage, MessageType};
use wasm_bindgen::runtime::{get_runtime, AppEvent};

use crate::FunctionRegistry;

/// Decode request data from the dioxus-data header.
fn decode_request_data(request: &wry::http::Request<Vec<u8>>) -> Option<IPCMessage> {
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
    ongoing_request: Option<RequestAsyncResponder>,
    /// Count of Evaluates we've sent to JS that we're still waiting for responses to
    pending_evaluates: usize,
}

impl SharedWebviewState {
    fn set_ongoing_request(&mut self, responder: RequestAsyncResponder) {
        self.ongoing_request = Some(responder);
    }

    fn take_ongoing_request(&mut self) -> Option<RequestAsyncResponder> {
        self.ongoing_request.take()
    }

    fn has_pending_request(&self) -> bool {
        self.ongoing_request.is_some()
    }

    fn is_in_conversation(&self) -> bool {
        self.has_pending_request() || self.pending_evaluates > 0
    }

    fn respond_to_request(&mut self, response: IPCMessage) {
        if let Some(responder) = self.take_ongoing_request() {
            let ty = response.ty().unwrap();
            if ty == MessageType::Evaluate {
                self.pending_evaluates += 1;
            }

            let body = response.into_data();
            responder.respond(
                wry::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "application/octet-stream")
                    .body(body)
                    .expect("Failed to build response"),
            );
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
    function_registry: &'static FunctionRegistry,
    shared: Rc<RefCell<SharedWebviewState>>,
    state: RefCell<WebviewLoadingState>,
}

impl WryBindgen {
    /// Create a new WryBindgen instance.
    ///
    /// # Arguments
    /// * `function_registry` - Reference to the collected JS function specifications
    pub fn new(function_registry: &'static FunctionRegistry) -> Self {
        Self {
            function_registry,
            shared: Rc::new(RefCell::new(SharedWebviewState::default())),
            state: RefCell::new(WebviewLoadingState::default()),
        }
    }

    /// Get the initialization script that must be evaluated in the webview.
    ///
    /// This script sets up the JavaScript function registry and IPC infrastructure.
    pub fn init_script(&self) -> &str {
        self.function_registry.script()
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
    pub fn create_protocol_handler<F, H>(
        &self,
        proxy: F,
        root_response: H,
    ) -> impl Fn(&wry::http::Request<Vec<u8>>, RequestAsyncResponder) + 'static
    where
        F: Fn(AppEvent) + 'static,
        H: Fn() -> wry::http::Response<Vec<u8>> + 'static,
    {
        let shared = self.shared.clone();
        let function_registry = self.function_registry;

        move |request: &wry::http::Request<Vec<u8>>, responder: RequestAsyncResponder| {
            let real_path = request.uri().to_string().replace("wry://", "");
            let real_path = real_path.as_str().trim_matches('/');

            if real_path == "index" {
                responder.respond(root_response());
                return;
            }

            if real_path == "ready" {
                proxy(AppEvent::WebviewLoaded);
                responder.respond(blank_response());
                return;
            }

            // Serve inline_js modules from snippets/
            if real_path.starts_with("snippets/") {
                if let Some(content) = function_registry.get_module(real_path) {
                    responder.respond(module_response(content));
                    return;
                }
                responder.respond(not_found_response());
                return;
            }

            let mut shared = shared.borrow_mut();
            let Some(msg) = decode_request_data(request) else {
                responder.respond(error_response());
                return;
            };

            if real_path == "handler" {
                let msg_type = msg.ty().unwrap();
                println!("got ipc message of type {:?}", msg_type);
                match msg_type {
                    MessageType::Evaluate => {
                        // New call from JS - save responder and wait for Rust to respond
                        shared.set_ongoing_request(responder);
                    }
                    MessageType::Respond => {
                        println!("shared.pending_evaluates = {}", shared.pending_evaluates);
                        // JS is sending a result back to Rust
                        if shared.is_in_conversation() {
                            // We're in the middle of a conversation (Rust sent an Evaluate,
                            // JS processed it, now sending the result). Save responder and
                            // wait for Rust to send the next message (another Evaluate or
                            // the final Respond).
                            if shared.pending_evaluates > 0 {
                                shared.pending_evaluates -= 1;
                            }
                            shared.set_ongoing_request(responder);
                        } else {
                            // No active conversation - this is the final result.
                            // Respond immediately with blank.
                            responder.respond(blank_response());
                        }
                    }
                }
                get_runtime().queue_rust_call(msg);
                println!("Queued Rust call");
                return;
            }

            responder.respond(blank_response());
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
    pub fn handle_user_event(&self, event: AppEvent, webview: &wry::WebView) -> Option<i32> {
        match event {
            AppEvent::Shutdown(status) => {
                return Some(status);
            }
            AppEvent::Ipc(ipc_msg) => {
                {
                    let mut state = self.state.borrow_mut();
                    if let WebviewLoadingState::Pending { queued } = &mut *state {
                        queued.push(ipc_msg);
                        return None;
                    }
                } // Release borrow before accessing shared

                let mut shared = self.shared.borrow_mut();

                if shared.has_pending_request() {
                    shared.respond_to_request(ipc_msg);
                    return None;
                }

                let decoded = ipc_msg.decoded().unwrap();

                if let DecodedVariant::Evaluate { .. } = decoded {
                    // Encode the binary data as base64 and pass to JS
                    // JS will iterate over operations in the buffer
                    let engine = base64::engine::general_purpose::STANDARD;
                    let data_base64 = engine.encode(ipc_msg.data());
                    let code = format!("window.evaluate_from_rust_binary(\"{}\")", data_base64);
                    webview.evaluate_script(&code).unwrap();
                }
            }
            AppEvent::WebviewLoaded => {
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
pub fn blank_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

/// Create an error HTTP response.
pub fn error_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

/// Create a JavaScript module HTTP response.
pub fn module_response(content: &str) -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

/// Create a not found HTTP response.
pub fn not_found_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}
