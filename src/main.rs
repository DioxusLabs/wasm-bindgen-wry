use base64::Engine;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::marker::PhantomData;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, OnceLock, RwLock, mpsc};
use winit::event_loop::EventLoopProxy;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, RequestAsyncResponder, WebViewBuilder};

// Message types for thread communication
enum IPCMessage {
    Evaluate {
        fn_id: u32,
        args: Vec<serde_json::Value>,
        result_sender: mpsc::Sender<Vec<u8>>,
    },
    Respond {
        id: DefaultKey,
        response: ResponseOrCall,
    },
    Shutdown,
}

struct Oneshot<T> {
    lock: Arc<OnceLock<T>>,
}

impl<T> Oneshot<T> {
    fn new() -> Self {
        Self {
            lock: Arc::new(OnceLock::new()),
        }
    }

    fn set(&self, value: T) {
        self.lock
            .set(value)
            .unwrap_or_else(|_| panic!("Oneshot value already set"));
    }

    fn take(&self) -> &T {
        self.lock.wait()
    }
}

struct OngoingRustCall {
    // The request associated with this call
    responder: RequestAsyncResponder,
}

#[derive(Serialize, Deserialize)]
enum ResponseOrCall {
    Response(serde_json::Value),
    Call {
        code: u64,
        args: Vec<serde_json::Value>,
    },
}

fn decode_request_data(request: &wry::http::Request<Vec<u8>>) -> Option<Vec<u8>> {
    if let Some(header_value) = request.headers().get("dioxus-data") {
        // Decode base64 header
        let engine = base64::engine::general_purpose::STANDARD;
        if let Ok(decoded_bytes) = engine.decode(header_value) {
            return Some(decoded_bytes);
        }
    }
    None
}

#[derive(Default)]
struct SharedWebviewState {
    pending_requests: SlotMap<DefaultKey, mpsc::Sender<Vec<u8>>>,
    ongoing_requests: SlotMap<DefaultKey, OngoingRustCall>,
}

impl SharedWebviewState {
    fn push_pending_request(&mut self, sender: mpsc::Sender<Vec<u8>>) -> DefaultKey {
        self.pending_requests.insert(sender)
    }

    fn finish_pending_request(&mut self, id: DefaultKey, response: Vec<u8>) {
        if let Some(sender) = self.pending_requests.remove(id) {
            _ = sender.send(response)
        }
    }

    fn push_ongoing_request(&mut self, ongoing: OngoingRustCall) -> DefaultKey {
        self.ongoing_requests.insert(ongoing)
    }

    fn respond_to_request(&mut self, id: DefaultKey, response: ResponseOrCall) {
        if let Some(ongoing) = self.ongoing_requests.remove(id) {
            ongoing.responder.respond(match response {
                ResponseOrCall::Response(value) => wry::http::Response::builder()
                    .header("Content-Type", "application/xml")
                    .body(serde_json::to_vec(&value).unwrap())
                    .map_err(|e| e.to_string())
                    .expect("Failed to build response"),
                ResponseOrCall::Call { code, args } => {
                    let serialized_args = serde_json::to_vec(&args).unwrap();
                    wry::http::Response::builder()
                        .header("Content-Type", "application/xml")
                        .body(
                            serde_json::to_vec(&serde_json::json!({
                                "code": code,
                                "args": serde_json::from_slice::<serde_json::Value>(&serialized_args).unwrap(),
                            }))
                            .unwrap(),
                        )
                        .map_err(|e| e.to_string())
                        .expect("Failed to build response")
                }
            });
        }
    }
}

fn error_response() -> wry::http::Response<Vec<u8>> {
    wry::http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

#[derive(Default)]
struct State {
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
                if real_path == "" {
                    responder.respond(root_response());
                    return;
                }
                println!("Handling request for path: {}", real_path);
                println!("Request: {:?}", request);
                let id: Option<DefaultKey> = request
                    .headers()
                    .get("dioxus-request-id")
                    .and_then(|v| {
                        v.as_bytes()
                            .iter()
                            .map(|b| *b as char)
                            .collect::<String>()
                            .parse::<u64>()
                            .ok()
                    })
                    .map(|v| KeyData::from_ffi(v).into());
                let Some(id) = id else {
                    responder.respond(error_response());
                    return;
                };
                let mut shared = shared.write().unwrap();
                let Some(data) = decode_request_data(&request) else {
                    responder.respond(error_response());
                    return;
                };
                if real_path == "handler" {
                    shared.finish_pending_request(id, data);
                } else if real_path == "callback" {
                    EVENT_LOOP_PROXY.get()
                        .expect("Event loop proxy not set")
                        .queue_rust_call(ResponseOrCall::Call {
                            code: id.data().as_ffi(),
                            args: serde_json::from_slice::<Vec<serde_json::Value>>(&data)
                                .unwrap(),
                        });
                }
            })
            .with_url("wry://local")
            .build_as_child(&window)
            .unwrap();

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

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: IPCMessage) {
        match event {
            IPCMessage::Evaluate {
                fn_id,
                args,
                result_sender,
            } => {
                fn format_call<'a>(
                    req_id: u64,
                    function_id: u32,
                    args: impl Iterator<Item = serde_json::Value>,
                ) -> String {
                    let mut call = String::new();
                    call.push_str("evaluate_from_rust(");
                    call.push_str(&req_id.to_string());
                    call.push_str(", ");
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
                let id = self
                    .shared
                    .write()
                    .unwrap()
                    .push_pending_request(result_sender);
                let code = format_call(id.data().as_ffi(), fn_id, args.into_iter());
                self.webview
                    .as_ref()
                    .unwrap()
                    .evaluate_script(&code)
                    .unwrap();
            }
            IPCMessage::Respond { id, response } => {
                self.shared
                    .write()
                    .unwrap()
                    .respond_to_request(id, response);
            }
            IPCMessage::Shutdown => {
                event_loop.exit();
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

struct DomEnv {
    proxy: EventLoopProxy<IPCMessage>,
    queued_rust_calls: RwLock<Vec<ResponseOrCall>>,
    sender: RwLock<Option<Sender<ResponseOrCall>>>,
}

impl DomEnv {
    fn new(proxy: EventLoopProxy<IPCMessage>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            sender: RwLock::new(None),
        }
    }

    fn queue_rust_call(&self, responder: ResponseOrCall) {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            let _ = sender.send(responder);
        } else {
            self.queued_rust_calls.write().unwrap().push(responder);
        }
    }

    fn set_sender(&self, sender: Sender<ResponseOrCall>) {
        let mut queued = self.queued_rust_calls.write().unwrap();
        *self.sender.write().unwrap() = Some(sender);
        for call in queued.drain(..) {
            if let Some(sender) = self.sender.read().unwrap().as_ref() {
                let _ = sender.send(call);
            }
        }
    }
}

static EVENT_LOOP_PROXY: OnceLock<DomEnv> = OnceLock::new();

struct ThreadLocalEncoder {
    encoder: RwLock<Encoder>,
}

thread_local! {
    static THREAD_LOCAL_ENCODER: ThreadLocalEncoder = ThreadLocalEncoder {
        encoder: RwLock::new(Encoder::new()),
    };
}

fn encode_in_thread_local<T: RustEncode<P>, P>(value: T) -> serde_json::Value {
    THREAD_LOCAL_ENCODER.with(|tle| {
        let mut encoder = tle.encoder.write().unwrap();
        encoder.encode(value)
    })
}

fn set_event_loop_proxy(proxy: EventLoopProxy<IPCMessage>) {
    EVENT_LOOP_PROXY
        .set(DomEnv::new(proxy))
        .unwrap_or_else(|_| panic!("Event loop proxy already set"));
}

fn get_event_loop_proxy() -> &'static EventLoopProxy<IPCMessage> {
    &EVENT_LOOP_PROXY
        .get()
        .expect("Event loop proxy not set")
        .proxy
}

fn main() -> wry::Result<()> {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    {
        use gtk::prelude::DisplayExtManual;

        gtk::init().unwrap();
        if gtk::gdk::Display::default().unwrap().backend().is_wayland() {
            panic!("This example doesn't support wayland!");
        }

        winit::platform::x11::register_xlib_error_hook(Box::new(|_display, error| {
            let error = error as *mut x11_dl::xlib::ErrorEvent;
            (unsafe { (*error).error_code }) == 170
        }));
    }

    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    set_event_loop_proxy(proxy);
    std::thread::spawn(app);
    let mut state = State::default();
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}

fn app() {
    let add_function = ADD_NUMBERS;
    let assert_sum_works = move || {
        let sum: i32 = add_function.call(5, 7);
        println!("Sum from JS: {}", sum);
        assert_eq!(sum, 12);
    };
    assert_sum_works();
    let add_event_listener: JSFunction<fn(_, _)> = JSFunction::new(3);
    add_event_listener.call("click".to_string(), move || {
        println!("Button clicked!");
        assert_sum_works();
    });
}

struct Encoder {
    functions: SlotMap<
        DefaultKey,
        Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync>,
    >,
}

impl Encoder {
    fn new() -> Self {
        Self {
            functions: SlotMap::new(),
        }
    }

    fn encode<T: RustEncode<P>, P>(&mut self, value: T) -> serde_json::Value {
        value.encode(self)
    }

    fn encode_function<T: IntoRustCallable<P>, P>(&mut self, function: T) -> serde_json::Value {
        let key = self.functions.insert(function.into());
        serde_json::json!({
            "type": "function",
            "id": key.data().as_ffi(),
        })
    }
}

trait RustEncode<P = ()> {
    fn encode(self, encoder: &mut Encoder) -> serde_json::Value;
}

impl RustEncode for String {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::String(self)
    }
}

impl RustEncode for () {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::Null
    }
}

impl RustEncode for i32 {
    fn encode(self, _encoder: &mut Encoder) -> serde_json::Value {
        serde_json::Value::Number(serde_json::Number::from(self))
    }
}

impl<F, P> RustEncode<P> for F
where
    F: IntoRustCallable<P>,
{
    fn encode(self, encoder: &mut Encoder) -> serde_json::Value {
        encoder.encode_function(self)
    }
}

trait IntoRustCallable<T> {
    fn into(self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync>;
}

impl<R, F> IntoRustCallable<fn() -> R> for F
where
    F: FnMut() -> R + Send + Sync + 'static,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |_: Vec<serde_json::Value>| {
            let result: R = (self)();
            serde_json::to_value(result).unwrap()
        })
    }
}

impl<T, R, F> IntoRustCallable<fn(T) -> R> for F
where
    F: FnMut(T) -> R + Send + Sync + 'static,
    T: for<'de> Deserialize<'de>,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |args: Vec<serde_json::Value>| {
            let mut args_iter = args.into_iter();
            let arg: T = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let result: R = (self)(arg);
            serde_json::to_value(result).unwrap()
        })
    }
}

impl<T1, T2, R, F> IntoRustCallable<fn(T1, T2) -> R> for F
where
    F: FnMut(T1, T2) -> R + Send + Sync + 'static,
    T1: for<'de> Deserialize<'de>,
    T2: for<'de> Deserialize<'de>,
    R: serde::Serialize,
{
    fn into(mut self) -> Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync> {
        Box::new(move |args: Vec<serde_json::Value>| {
            let mut args_iter = args.into_iter();
            let arg1: T1 = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let arg2: T2 = serde_json::from_value(args_iter.next().unwrap()).unwrap();
            let result: R = (self)(arg1, arg2);
            serde_json::to_value(result).unwrap()
        })
    }
}

const CONSOLE_LOG: JSFunction<fn(String)> = JSFunction::new(0);
const ALERT: JSFunction<fn(String)> = JSFunction::new(1);
const ADD_NUMBERS: JSFunction<fn(i32, i32) -> i32> = JSFunction::new(2);
const ADD_EVENT_LISTENER: JSFunction<fn(String, fn())> = JSFunction::new(3);

struct JSFunction<T> {
    id: u32,
    function: PhantomData<T>,
}

impl<T> JSFunction<T> {
    const fn new(id: u32) -> Self {
        Self {
            id,
            function: PhantomData,
        }
    }
}

impl<T, R> JSFunction<fn(T) -> R> {
    fn call<P>(&self, args: T) -> R
    where
        T: RustEncode<P>,
        R: DeserializeOwned,
    {
        let args_json = encode_in_thread_local(args);
        run_js_sync(get_event_loop_proxy(), self.id, vec![args_json])
    }
}

impl<T1, T2, R> JSFunction<fn(T1, T2) -> R> {
    fn call<P1, P2>(&self, arg1: T1, arg2: T2) -> R
    where
        T1: RustEncode<P1>,
        T2: RustEncode<P2>,
        R: DeserializeOwned,
    {
        let arg1_json = encode_in_thread_local(arg1);
        let arg2_json = encode_in_thread_local(arg2);
        run_js_sync(get_event_loop_proxy(), self.id, vec![arg1_json, arg2_json])
    }
}

fn run_js_sync<T: DeserializeOwned>(
    proxy: &EventLoopProxy<IPCMessage>,
    fn_id: u32,
    args: Vec<serde_json::Value>,
) -> T {
    let (result_sender, result_receiver) = mpsc::channel();
    _ = proxy.send_event(IPCMessage::Evaluate {
        fn_id,
        args,
        result_sender,
    });

    let mut result_bytes = result_receiver.recv().unwrap();
    if result_bytes.is_empty() {
        result_bytes = b"null".to_vec();
    }
    serde_json::from_slice(&result_bytes).unwrap()
}

fn root_response() -> wry::http::Response<Vec<u8>> {
    // Serve the main HTML page
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Wry Test</title>
</head>
<body>
    <h1>Wry Custom Protocol Test</h1>
    <button onclick="testRequest()">Test XML Request</button>
    <div id="result"></div>

    <script>
        function testRequest() {
            sync_request("wry://handler", { event: "test_event" })
                .then(response => {
                    document.getElementById("result").innerText = "Response: " + JSON.stringify(response);
                });
        }
        // This function sends the event to the virtualdom and then waits for the virtualdom to process it
        //
        // However, it's not really suitable for liveview, because it's synchronous and will block the main thread
        // We should definitely consider using a websocket if we want to block... or just not block on liveview
        // Liveview is a little bit of a tricky beast
        function sync_request(endpoint, contents) {
            // Handle the event on the virtualdom and then process whatever its output was
            const xhr = new XMLHttpRequest();

            // Serialize the event and send it to the custom protocol in the Rust side of things
            xhr.open("POST", endpoint, false);
            xhr.setRequestHeader("Content-Type", "application/json");

            // hack for android since we CANT SEND BODIES (because wry is using shouldInterceptRequest)
            //
            // https://issuetracker.google.com/issues/119844519
            // https://stackoverflow.com/questions/43273640/android-webviewclient-how-to-get-post-request-body
            // https://developer.android.com/reference/android/webkit/WebViewClient#shouldInterceptRequest(android.webkit.WebView,%20android.webkit.WebResourceRequest)
            //
            // the issue here isn't that big, tbh, but there's a small chance we lose the event due to header max size (16k per header, 32k max)
            const json_string = JSON.stringify(contents);
            const contents_bytes = new TextEncoder().encode(json_string);
            const contents_base64 = btoa(String.fromCharCode.apply(null, contents_bytes));
            xhr.setRequestHeader("dioxus-data", contents_base64);
            xhr.send();
        }

        function run_code(code, args) {
            let f;
            switch (code) {
                case 0:
                    f = console.log;
                    break;
                case 1:
                    f = alert;
                    break;
                case 2:
                    f = function(a, b) { return a + b; };
                    break;
                case 3:
                    f = function(event_name, callback) {
                        document.addEventListener(event_name, function(e) {
                            callback.call();
                        });
                    };
                    break;
                default:
                    throw new Error("Unknown code: " + code);
            }
            return f.apply(null, args);
        }

        function evaluate_from_rust(id, code, args_json) {
            let args = deserialize_args(args_json);
            const result = run_code(code, args);
            const response = {
                id,
                result: result
            };
            sync_request("wry://handler", response);
        }

        function deserialize_args(args_json) {
            if (typeof args_json === "string") {
                return args_json;
            } else if (typeof args_json === "number") {
                return args_json;
            } else if (Array.isArray(args_json)) {
                return args_json.map(deserialize_args);
            } else if (typeof args_json === "object" && args_json !== null) {
                if (args_json.type === "function") {
                    return new RustFunction(args_json.id);
                } else {
                    const obj = {};
                    for (const key in args_json) {
                        obj[key] = deserialize_args(args_json[key]);
                    }
                    return obj;
                }
            }
        }

        class RustFunction {
            constructor(code) {
                this.code = code;
            }

            call(...args) {
                return sync_request("wry://callback", {
                    code: this.code,
                    args: args
                });
            }
        }
    </script>
</body>
</html>"#;

    wry::http::Response::builder()
        .header("Content-Type", "text/html")
        .body(html.as_bytes().to_vec())
        .map_err(|e| e.to_string())
        .expect("Failed to build response")
}
