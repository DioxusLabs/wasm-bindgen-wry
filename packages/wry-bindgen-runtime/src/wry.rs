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
use core::pin::Pin;
use core::task::Poll;

use http::Response;

use crate::batch::{Runtime, in_runtime};
use crate::function_registry::FUNCTION_REGISTRY;
use crate::ipc::{IPCMessage, decode_data};
use crate::runtime::{
    DriverCommand, DriverCommandReceiver, DriverCommandSender, DriverCommandWeakSender, IPCSenders,
    Inbound, InboundSendError, WryIPC, dispatch_inbound_message,
};

struct WryBindgenResponder {
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
    /// Webview is still loading. The lock protocol permits at most one Rust IPC
    /// message to be waiting for load, though normally this remains empty
    /// because user code cannot run until the lock can be acquired.
    Pending {
        pending_ipc: Option<IPCMessage>,
        acquire_lock: bool,
    },
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

    fn handle_ipc_message(&mut self, ipc_msg: IPCMessage) {
        if let WebviewLoadingState::Pending { pending_ipc, .. } = &mut self.loading_state {
            assert!(
                pending_ipc.replace(ipc_msg).is_none(),
                "multiple Rust IPC messages queued before webview load"
            );
            return;
        }

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
    }

    fn mark_initialized(&mut self) -> bool {
        match self.loading_state {
            WebviewLoadingState::Pending { .. } => self.mark_loaded(),
            WebviewLoadingState::Loaded => true,
        }
    }
}

enum DriverAction {
    None,
    RequestJsLock,
}

impl DriverAction {
    fn run(self, evaluate_script: &mut impl FnMut(&str)) {
        match self {
            DriverAction::None => {}
            DriverAction::RequestJsLock => {
                evaluate_script("window.__wry_acquire_handler_lock()");
            }
        }
    }
}

impl WebviewMessageLayer {
    fn new(sender: IPCSenders) -> Self {
        Self {
            current_xhr: None,
            sender,
        }
    }

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

    fn receive_rust_message(&mut self, ipc_msg: IPCMessage) {
        // Deliver as the response to the parked JS XHR. This is the only
        // Rust->JS payload path; `evaluate_script` is reserved for asking JS to
        // acquire this lock.
        let responder = self.take_parked_xhr();
        responder.respond_ipc(ipc_msg);
    }

    fn release_lock(&mut self) {
        let responder = self.take_parked_xhr();
        responder.respond(blank_response());
    }

    /// Take the JS XHR currently parked in this layer. Every caller runs only
    /// while Rust holds the lock, so an XHR is always suspended here.
    fn take_parked_xhr(&mut self) -> WryBindgenResponder {
        self.current_xhr.take().unwrap()
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
    /// - "/__wbg__/preinitialized" - enables startup calls before the app lock
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
            // Inventory-collected modules first, then any registered at runtime by
            // `link_to!` (whose content is only known once the macro call runs).
            if let Some(content) = FUNCTION_REGISTRY
                .get_module(path_without_snippets)
                .or_else(|| crate::function_registry::linked_module(path_without_snippets))
            {
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

        if path_without_wbg == "preinitialized" {
            let _ = webviews.borrow_mut().mark_loaded();
            let responder = WryBindgenResponder::from(responder);
            responder.respond(blank_response());
            return None;
        }

        if path_without_wbg == "initialized" {
            let acquire_lock = webviews.borrow_mut().mark_initialized();
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
            }
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
    webview: Rc<RefCell<WebviewState>>,
    ipc: WryIPC,
    driver_commands: DriverCommandReceiver,
    weak_driver_commands: DriverCommandWeakSender,
}

impl WryBindgen {
    /// Create a new per-webview session.
    pub fn new() -> Self {
        let (ipc, senders, driver_commands) = WryIPC::new();
        let weak_driver_commands = ipc.command_sender().downgrade();
        Self {
            webview: Rc::new(RefCell::new(WebviewState::new(senders))),
            ipc,
            driver_commands,
            weak_driver_commands,
        }
    }

    /// Get the protocol handler for this webview.
    pub fn protocol_handler(&self) -> ProtocolHandler {
        ProtocolHandler {
            webview: self.webview.clone(),
            driver_commands: self.weak_driver_commands.clone(),
        }
    }

    /// Split the session into an app-thread runtime endpoint and a main-thread driver.
    pub fn split(self) -> (WryBindgenRuntime, WryBindgenDriver) {
        (
            WryBindgenRuntime { ipc: self.ipc },
            WryBindgenDriver {
                webview: self.webview,
                commands: self.driver_commands,
            },
        )
    }
}

impl Default for WryBindgen {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for a held JS lock.
///
/// Holding the guard means a JS XHR is parked, suspending the JS event loop so
/// Rust can drive JS. Dropping it replies to that XHR, handing control back to
/// the JS event loop until the next wake. Tying release to the guard's scope
/// keeps acquire and release paired.
struct JsLockGuard {
    commands: DriverCommandSender,
}

impl JsLockGuard {
    fn acquire(ipc: &WryIPC) -> Self {
        Self {
            commands: ipc.command_sender(),
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

impl WryBindgenRuntime {
    /// Build a sendable wrapper that creates and runs the app future on the
    /// thread where it is awaited.
    pub fn run<F, Fut>(
        self,
        app: F,
    ) -> impl IntoFuture<Output = (), IntoFuture: 'static> + Send + 'static
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: core::future::Future<Output = ()> + 'static,
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
        }

        RuntimeFuture::new(app, self.ipc)
    }
}

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

/// Main-thread driver bound to the webview's script evaluator.
pub struct WryBindgenWebviewDriver {
    driver: WryBindgenDriver,
    evaluate_script: Box<dyn FnMut(&str)>,
}

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
        }
    }
}

/// Create a blank HTTP response.
fn blank_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

/// Create an error HTTP response.
fn error_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

/// Create a JavaScript module HTTP response.
fn module_response(content: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .header("access-control-allow-origin", "*")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

/// Create a not found HTTP response.
fn not_found_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{DecodedVariant, MessageType};
    use std::sync::Arc;

    fn ipc_message(message_type: MessageType) -> IPCMessage {
        crate::ipc::empty_message(message_type)
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
    fn lock_request_is_queued_until_webview_loads() {
        let bindgen = WryBindgen::new();
        let protocol_handler = bindgen.protocol_handler();

        let evaluated_scripts = Rc::new(RefCell::new(Vec::new()));
        let captured_scripts = evaluated_scripts.clone();
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
        );
    }

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

        let request = initialized_request();
        let unhandled = protocol_handler.handle_request("wry", &request, |_| {});
        assert!(unhandled.is_none());

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = handler_request(MessageType::Evaluate);

        let unhandled = protocol_handler.handle_request("wry", &request, move |response| {
            *captured_response.borrow_mut() = Some(response)
        });

        assert!(unhandled.is_none());
        assert!(
            response.borrow().is_none(),
            "JS callback XHR should stay parked while Rust handles it"
        );

        runtime.ipc.send_acquire_lock();
        assert!(matches!(poll_driver(&mut driver), Poll::Pending));
        assert_eq!(
            evaluated_scripts.borrow().as_slice(),
            ["window.__wry_acquire_handler_lock()"],
            "lock script should be requested while the parked XHR is outstanding"
        );

        runtime.ipc.send_ipc(ipc_message(MessageType::Respond));
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
