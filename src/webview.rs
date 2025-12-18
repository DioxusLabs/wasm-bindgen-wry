use base64::Engine;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, RequestAsyncResponder, WebViewBuilder};

use wasm_bindgen::ipc::{DecodedVariant, IPCMessage, MessageType, decode_data};
use wasm_bindgen::runtime::{AppEvent, get_runtime};

use crate::FunctionRegistry;
use crate::home::root_response;

fn decode_request_data(request: &wry::http::Request<Vec<u8>>) -> Option<IPCMessage> {
    if let Some(header_value) = request.headers().get("dioxus-data") {
        return decode_data(header_value.as_bytes());
    }
    None
}

enum WebviewLoadingState {
    Pending {
        queued: Vec<IPCMessage>,
    },
    Loaded,
}

impl Default for WebviewLoadingState {
    fn default() -> Self {
        WebviewLoadingState::Pending { queued: Vec::new() }
    }
}

pub(crate) struct State {
    function_registry: &'static FunctionRegistry,
    window: Option<Window>,
    webview: Option<wry::WebView>,
    shared: Rc<RefCell<SharedWebviewState>>,
    state: WebviewLoadingState,
    proxy: EventLoopProxy<AppEvent>,
}

impl State {
    pub fn new(function_registry: &'static FunctionRegistry, proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            function_registry,
            window: None,
            webview: None,
            shared: Rc::new(RefCell::new(SharedWebviewState::default())),
            state: WebviewLoadingState::default(),
            proxy,
        }
    }
}

impl ApplicationHandler<AppEvent> for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = Window::default_attributes();
        attributes.inner_size = Some(LogicalSize::new(800, 800).into());
        let window = event_loop.create_window(attributes).unwrap();
        let shared = self.shared.clone();
        let proxy = self.proxy.clone();

        let webview = WebViewBuilder::new()
            .with_devtools(true)
            .with_asynchronous_custom_protocol("wry".into(), move |_, request, responder| {
                // path is the string slice, request is the Request object
                let real_path = request.uri().to_string().replace("wry://", "");
                let real_path = real_path.as_str().trim_matches('/');

                if real_path == "index" {
                    responder.respond(root_response());
                    return;
                }

                if real_path == "ready" {
                    proxy.send_event(AppEvent::WebviewLoaded).unwrap();
                    responder.respond(blank_response());
                    return;
                }

                // Serve inline_js modules from snippets/
                if real_path.starts_with("snippets/") {
                    if let Some(content) = crate::FUNCTION_REGISTRY.get_module(real_path) {
                        responder.respond(module_response(content));
                        return;
                    }
                    responder.respond(not_found_response());
                    return;
                }

                let mut shared = shared.borrow_mut();
                let Some(msg) = decode_request_data(&request) else {
                    responder.respond(error_response());
                    return;
                };
                if real_path == "handler" {
                    match msg.ty().unwrap() {
                        MessageType::Evaluate => {
                            shared.push_ongoing_request(OngoingRustCall { responder });
                        }
                        _ => {
                            shared.ongoing_request = OngoingRequestState::Completed;
                            responder.respond(blank_response());
                        }
                    }
                    get_runtime().queue_rust_call(msg);
                    return;
                }

                responder.respond(blank_response());
            })
            .with_url("wry://index")
            .build_as_child(&window)
            .unwrap();

        webview
            .evaluate_script(self.function_registry.script())
            .unwrap();
        webview.open_devtools();

        self.window = Some(window);
        self.webview = Some(webview);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let window = self.window.as_ref().unwrap();
                let webview = self.webview.as_ref().unwrap();

                let size = size.to_logical::<u32>(window.scale_factor());
                webview
                    .set_bounds(Rect {
                        position: LogicalPosition::new(0, 0).into(),
                        size: LogicalSize::new(size.width, size.height).into(),
                    })
                    .unwrap();
            }
            WindowEvent::CloseRequested => {
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Shutdown(status) => {
                event_loop.exit();
                std::process::exit(status);
            }
            AppEvent::Ipc(ipc_msg) => {
                if let WebviewLoadingState::Pending { .. } = &self.state {
                    if let WebviewLoadingState::Pending { queued } = &mut self.state {
                        queued.push(ipc_msg);
                    }
                    return;
                }

                let mut shared = self.shared.borrow_mut();

                if let OngoingRequestState::Pending(_) = &shared.ongoing_request {
                    shared.respond_to_request(ipc_msg);
                    return;
                }

                let decoded = ipc_msg.decoded().unwrap();

                if let DecodedVariant::Evaluate { .. } = decoded {
                    // Encode the binary data as base64 and pass to JS
                    // JS will iterate over operations in the buffer
                    let engine = base64::engine::general_purpose::STANDARD;
                    let data_base64 = engine.encode(ipc_msg.data());
                    let code = format!("window.evaluate_from_rust_binary(\"{}\")", data_base64);
                    self.webview
                        .as_ref()
                        .unwrap()
                        .evaluate_script(&code)
                        .unwrap();
                }
            }
            AppEvent::WebviewLoaded => {
                if let WebviewLoadingState::Pending { queued } =
                    std::mem::replace(&mut self.state, WebviewLoadingState::Loaded)
                {
                    for msg in queued {
                        get_runtime().js_response(msg);
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        ))]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }
    }
}

#[derive(Default)]
struct SharedWebviewState {
    ongoing_request: OngoingRequestState,
}

impl SharedWebviewState {
    fn push_ongoing_request(&mut self, ongoing: OngoingRustCall) {
        self.ongoing_request = OngoingRequestState::Pending(ongoing.responder);
    }

    fn respond_to_request(&mut self, response: IPCMessage) {
        if let OngoingRequestState::Pending(responder) = self.ongoing_request.take() {
            let ty = response.ty().unwrap();
            self.ongoing_request = match ty {
                MessageType::Evaluate => OngoingRequestState::Querying,
                MessageType::Respond => OngoingRequestState::Completed,
            };

            // Send binary response data
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

fn error_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

fn blank_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

fn module_response(content: &str) -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

fn not_found_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}

struct OngoingRustCall {
    // The request associated with this call
    responder: RequestAsyncResponder,
}

#[derive(Default)]
enum OngoingRequestState {
    Pending(RequestAsyncResponder),
    Querying,
    #[default]
    Completed,
}

impl Debug for OngoingRequestState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OngoingRequestState::Pending(_) => write!(f, "Pending"),
            OngoingRequestState::Querying => write!(f, "Querying"),
            OngoingRequestState::Completed => write!(f, "Completed"),
        }
    }
}

impl OngoingRequestState {
    fn take(&mut self) -> OngoingRequestState {
        std::mem::replace(self, OngoingRequestState::Completed)
    }
}
