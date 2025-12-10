use base64::Engine;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, Key, KeyData, SlotMap};
use std::fmt::Debug;
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
#[derive(Serialize, Deserialize, Debug)]
enum IPCMessage {
    Evaluate {
        fn_id: u64,
        args: Vec<serde_json::Value>,
    },
    Respond {
        response: serde_json::Value,
    },
    Shutdown,
}

struct OngoingRustCall {
    // The request associated with this call
    responder: RequestAsyncResponder,
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
            println!("Responding to ongoing request with response: {:?}", response);
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
                println!("Handling request for path: {}", real_path);
                println!("Request: {:?}", request);
                if real_path == "index" {
                    responder.respond(root_response());
                    return;
                }
                let mut shared = shared.write().unwrap();
                let Some(data) = decode_request_data(&request) else {
                    responder.respond(error_response());
                    return;
                };
                if real_path == "handler" {
                    match serde_json::from_slice::<IPCMessage>(&data) {
                        Ok(msg) => {
                            match &msg {
                                IPCMessage::Evaluate { .. } => {
                                    shared.push_ongoing_request(OngoingRustCall { responder });
                                }
                                _ => {
                                    shared.ongoing_request = OngoingRequestState::Completed;
                                    responder.respond(blank_response());
                                }
                            }
                            EVENT_LOOP_PROXY
                                .get()
                                .expect("Event loop proxy not set")
                                .queue_rust_call(msg);
                        }
                        Err(e) => println!("Failed to decode IPCMessage: {}", e),
                    }
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
        match &shared.ongoing_request {
            OngoingRequestState::Pending(_) => {
                shared.respond_to_request(event);
                return;
            }
            _ => {}
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

struct DomEnv {
    proxy: EventLoopProxy<IPCMessage>,
    queued_rust_calls: RwLock<Vec<IPCMessage>>,
    sender: RwLock<Option<Sender<IPCMessage>>>,
}

impl DomEnv {
    fn new(proxy: EventLoopProxy<IPCMessage>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            sender: RwLock::new(None),
        }
    }

    fn js_response(&self, responder: IPCMessage) {
        let _ = self.proxy.send_event(responder);
    }

    fn queue_rust_call(&self, responder: IPCMessage) {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            let _ = sender.send(responder);
        } else {
            self.queued_rust_calls.write().unwrap().push(responder);
        }
    }

    fn set_sender(&self, sender: Sender<IPCMessage>) {
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
    receiver: Receiver<IPCMessage>,
}

thread_local! {
    static THREAD_LOCAL_ENCODER: ThreadLocalEncoder = ThreadLocalEncoder {
        encoder: RwLock::new(Encoder::new()),
        receiver: {
            let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
            let (sender, receiver) = mpsc::channel();
            env.set_sender(sender);
            receiver
        },
    };
}

fn encode_in_thread_local<T: RustEncode<P>, P>(value: T) -> serde_json::Value {
    println!("Encoding value in Rust...");
    THREAD_LOCAL_ENCODER.with(|tle| {
        println!("Encoding value in thread local...");
        let mut encoder = tle.encoder.write().unwrap();
        println!("Got encoder lock...");
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
    let add_function = ADD_NUMBERS_JS;
    let set_text_content = SET_TEXT_CONTENT;
    let assert_sum_works = move || {
        println!("calling add_function from JS...");
        let sum: i32 = add_function.call(5, 7);
        println!("Sum from JS: {}", sum);
        assert_eq!(sum, 12);
    };
    assert_sum_works();
    println!("Setting up event listener...");
    let add_event_listener: JSFunction<fn(_, _)> = JSFunction::new(3);
    let mut count = 0;
    add_event_listener.call("click".to_string(), move || {
        println!("Button clicked!");
        assert_sum_works();
        count += 1;
        let new_text = format!("Button clicked {} times", count);
        set_text_content.call("click-count".to_string(), new_text);
    });
    wait_for_js_event::<()>();
}

struct Encoder {
    functions: SlotMap<
        DefaultKey,
        Option<Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + Sync>>,
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
        let key = self.functions.insert(Some(function.into()));
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
const ADD_NUMBERS_JS: JSFunction<fn(i32, i32) -> i32> = JSFunction::new(2);
const ADD_EVENT_LISTENER: JSFunction<fn(String, fn())> = JSFunction::new(3);
const SET_TEXT_CONTENT: JSFunction<fn(String, String)> = JSFunction::new(4);

struct JSFunction<T> {
    id: u64,
    function: PhantomData<T>,
}

impl<T> JSFunction<T> {
    const fn new(id: u64) -> Self {
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
    fn_id: u64,
    args: Vec<serde_json::Value>,
) -> T {
    println!("Sending JS evaluate request...");
    _ = proxy.send_event(IPCMessage::Evaluate { fn_id, args });

    wait_for_js_event()
}

fn wait_for_js_event<T: DeserializeOwned>() -> T {
    let env = EVENT_LOOP_PROXY.get().expect("Event loop proxy not set");
    THREAD_LOCAL_ENCODER.with(|tle| {
        println!("Waiting for JS response...");
        while let Some(response) = tle.receiver.recv().ok() {
            println!("Received response: {:?}", response);
            match response {
                IPCMessage::Respond { response } => {
                    println!("Got response from JS: {:?}", response);
                    return serde_json::from_value(response).unwrap();
                }
                IPCMessage::Evaluate { fn_id, args } => {
                    let key = KeyData::from_ffi(fn_id).into();
                    let function = {
                        let mut encoder = tle.encoder.write().unwrap();
                        encoder.functions.get_mut(key).map(|f| f.take().expect("function cannot be called recursively"))
                    };
                    if let Some(mut function) =
                        function
                    {
                        let result = function(args);
                        env.js_response(IPCMessage::Respond { response: result });
                        // Insert it back
                        let mut encoder = tle.encoder.write().unwrap();
                        encoder.functions.get_mut(key).unwrap().replace(function);
                    }
                }
                IPCMessage::Shutdown => {
                    panic!()
                }
            }
        }
        panic!()
    })
}

fn root_response() -> wry::http::Response<Vec<u8>> {
    // Serve the main HTML page
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Wry Test</title>
</head>
<body>
    <h1 id="click-count">Button not clicked yet</h1>

    <script>
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
            console.log("Sending request to Rust:", json_string);
            const contents_bytes = new TextEncoder().encode(json_string);
            const contents_base64 = btoa(String.fromCharCode.apply(null, contents_bytes));
            xhr.setRequestHeader("dioxus-data", contents_base64);
            xhr.send();

            const response_text = xhr.responseText;
            console.log("Received response from Rust:", response_text);
            try {
                return JSON.parse(response_text);
            } catch (e) {
                console.error("Failed to parse response JSON:", e);
                return null;
            }
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
                case 4:
                    f = function(element_id, text_content) {
                        const element = document.getElementById(element_id);
                        if (element) {
                            element.textContent = text_content;
                        } else {
                            console.warn("Element with ID " + element_id + " not found.");
                        }
                    };
                    break;
                default:
                    throw new Error("Unknown code: " + code);
            }
            return f.apply(null, args);
        }

        function evaluate_from_rust(code, args_json) {
            let args = deserialize_args(args_json);
            const result = run_code(code, args);
            const response = {
                Respond: {
                    response: result || null
                }
            };
            const request_result = sync_request("wry://handler", response);
            return handleResponse(request_result);
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

        function handleResponse(response) {
            if (!response) {
                return;
            }
            console.log("Handling response:", response);
            if (response.Respond) {
                return response.Respond.response;
            } else if (response.Evaluate) {
                return evaluate_from_rust(response.Evaluate.fn_id, response.Evaluate.args);
            }
            else {
                throw new Error("Unknown response type");
            }
        }

        class RustFunction {
            constructor(code) {
                this.code = code;
            }

            call(...args) {
                const response = sync_request("wry://handler", {
                    Evaluate: {
                        fn_id: this.code,
                        args: args
                    }
                });
                return handleResponse(response);
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
