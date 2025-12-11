use std::fmt::Debug;
use std::sync::{Arc, RwLock};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, RequestAsyncResponder, WebViewBuilder};

use crate::encoder::get_dom;
use crate::ipc::{IPCMessage, decode_data};
use crate::home::root_response;

fn decode_request_data(request: &wry::http::Request<Vec<u8>>) -> Option<IPCMessage> {
    if let Some(header_value) = request.headers().get("dioxus-data") {
        return decode_data(header_value.as_bytes());
    }
    None
}
#[derive(Default)]
pub(crate) struct State {
    window: Option<Window>,
    webview: Option<wry::WebView>,
    shared: Arc<RwLock<SharedWebviewState>>,
}

impl ApplicationHandler<IPCMessage> for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = Window::default_attributes();
        attributes.inner_size = Some(LogicalSize::new(800, 800).into());
        let window = event_loop.create_window(attributes).unwrap();
        let shared = self.shared.clone();

        let webview = WebViewBuilder::new()
            .with_asynchronous_custom_protocol("wry".into(), move |_, request, responder| {
                // path is the string slice, request is the Request object
                let real_path = request.uri().to_string().replace("wry://", "");
                let real_path = real_path.as_str().trim_matches('/');
                println!("Handling request for path: {}", real_path);
                println!("Request: {:?}", request);
                if real_path == "index" {
                    responder.respond(root_response());
                    return;
                }
                let mut shared = shared.write().unwrap();
                let Some(msg) = decode_request_data(&request) else {
                    responder.respond(error_response());
                    return;
                };
                if real_path == "handler" {
                    match &msg {
                        IPCMessage::Evaluate { .. } => {
                            shared.push_ongoing_request(OngoingRustCall { responder });
                        }
                        _ => {
                            shared.ongoing_request = OngoingRequestState::Completed;
                            responder.respond(blank_response());
                        }
                    }
                    get_dom().queue_rust_call(msg);
                    return;
                }

                responder.respond(blank_response());
            })
            .with_url("wry://index")
            .build_as_child(&window)
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
            _ => {
                // println!("got event\n{:#?}", event);
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: IPCMessage) {
        let mut shared = self.shared.write().unwrap();
        println!("Received IPCMessage: {:?}", event);
        println!("Ongoing request state: {:?}", shared.ongoing_request);
        if let OngoingRequestState::Pending(_) = &shared.ongoing_request {
            shared.respond_to_request(event);
            return;
        }

        if let IPCMessage::Evaluate { fn_id, args } = event {
            fn format_call<'a>(
                function_id: u64,
                args: impl Iterator<Item = serde_json::Value>,
            ) -> String {
                let mut call = String::new();
                call.push_str("evaluate_from_rust(");
                call.push_str(&function_id.to_string());
                call.push_str(", [");
                for (i, arg) in args.enumerate() {
                    if i > 0 {
                        call.push_str(", ");
                    }
                    call.push_str(&arg.to_string());
                }
                call.push_str("])");
                call
            }
            let code = format_call(fn_id, args.into_iter());
            self.webview
                .as_ref()
                .unwrap()
                .evaluate_script(&code)
                .unwrap();
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
        println!("Responding to request with response: {:?}", response);
        if let OngoingRequestState::Pending(responder) = self.ongoing_request.take() {
            if let IPCMessage::Evaluate { .. } = response {
                self.ongoing_request = OngoingRequestState::Querying;
            } else {
                self.ongoing_request = OngoingRequestState::Completed;
            }
            println!(
                "Responding to ongoing request with response: {:?}",
                response
            );
            responder.respond(
                wry::http::Response::builder()
                    .status(200)
                    .body(serde_json::to_vec(&response).unwrap())
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
