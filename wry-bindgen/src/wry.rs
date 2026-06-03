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
use core::pin::Pin;
use core::sync::atomic::AtomicU64;
use core::task::Poll;
use std::collections::HashMap;
use std::sync::Arc;

use http::Response;

use crate::batch::{Runtime, in_runtime};
use crate::function_registry::FUNCTION_REGISTRY;
use crate::ipc::{IPCMessage, OutboundIPCMessage, decode_data};
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
        pending_ipc: Option<OutboundIPCMessage>,
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
    // A function that evaluates scripts in the webview
    evaluate_script: Box<dyn FnMut(&str)>,
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
    fn new(sender: IPCSenders, evaluate_script: impl FnMut(&str) + 'static) -> Self {
        Self {
            messages: WebviewMessageLayer::new(sender),
            loading_state: WebviewLoadingState::default(),
            evaluate_script: Box::new(evaluate_script),
        }
    }

    fn evaluate_script(&mut self, script: &str) {
        (self.evaluate_script)(script);
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

    fn receive_rust_message(&mut self, ipc_msg: OutboundIPCMessage) {
        // Deliver as the response to the parked JS XHR. This is the only
        // Rust->JS payload path; `evaluate_script` is reserved for asking JS to
        // acquire this lock.
        let responder = self.take_parked_xhr();
        responder.respond_ipc(ipc_msg.message);
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
    max_id: AtomicU64,
    // State that is unique to each webview
    webview: Rc<RefCell<HashMap<u64, WebviewState>>>,
}

impl WryBindgen {
    /// Create a new WryBindgen instance.
    pub fn new(event_loop_proxy: impl Fn(WryBindgenEvent) + Send + Sync + 'static) -> Self {
        Self {
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
        let (ipc, senders) = WryIPC::new(event_loop_proxy);
        let id = self
            .max_id
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        self.webview.borrow_mut().insert(
            id,
            WebviewState::new(senders, |_| {
                unreachable!("evaluate_script will only be used after spawning the app")
            }),
        );

        AppBuilder {
            webview_id: id,
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
        }
    }

    fn handle_ipc_message(&self, id: u64, ipc_msg: OutboundIPCMessage) {
        self.with_webview_state(id, |webview_state| {
            if let WebviewLoadingState::Pending { pending_ipc, .. } =
                &mut webview_state.loading_state
            {
                assert!(
                    pending_ipc.replace(ipc_msg).is_none(),
                    "multiple Rust IPC messages queued before webview load"
                );
                return;
            }

            self.immediately_handle_ipc_message(webview_state, ipc_msg);
        });
    }

    fn with_webview_state(&self, id: u64, f: impl FnOnce(&mut WebviewState)) {
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

    fn request_js_lock(&self, webview_state: &mut WebviewState) {
        webview_state.evaluate_script("window.__wry_acquire_handler_lock()");
    }

    fn request_js_lock(&self, webview_state: &mut WebviewState) {
        webview_state.evaluate_script("window.__wry_acquire_handler_lock()");
    }

    fn immediately_handle_ipc_message(
        &self,
        webview_state: &mut WebviewState,
        ipc_msg: OutboundIPCMessage,
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
    fn acquire(ipc: &WryIPC, webview_id: u64) -> Self {
        Self {
            proxy: ipc.proxy.clone(),
            webview_id,
        }
    }
}

impl Drop for JsLockGuard {
    fn drop(&mut self) {
        (self.proxy)(WryBindgenEvent::release_lock(self.webview_id));
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
    pub fn build<F, F2, F3>(
        self,
        app: F,
        evaluate_script: F2,
    ) -> impl IntoFuture<Output = (), IntoFuture: 'static> + Send + 'static
    where
        F: FnOnce() -> F3 + Send + 'static,
        F2: FnMut(&str) + 'static,
        F3: core::future::Future<Output = ()> + 'static,
    {
        // First set up the evaluate_script function in the webview state
        {
            let mut webviews = self.bindgen.webview.borrow_mut();
            let webview_state = webviews
                .get_mut(&self.webview_id)
                .expect("The webview state was created in WryBindgen::spawner");
            webview_state.evaluate_script = Box::new(evaluate_script);
        }

        struct BuildFuture<F, F2> {
            app: F,
            webview_id: u64,
            ipc: WryIPC,
            phantom: core::marker::PhantomData<fn(F2)>,
        }

        impl<F, F2> BuildFuture<F, F2> {
            fn new(app: F, webview_id: u64, ipc: WryIPC) -> Self {
                Self {
                    app,
                    webview_id,
                    ipc,
                    phantom: core::marker::PhantomData,
                }
            }
        }

        impl<F, F2> IntoFuture for BuildFuture<F, F2>
        where
            F: FnOnce() -> F2 + Send + 'static,
            F2: core::future::Future<Output = ()> + 'static,
        {
            type IntoFuture = Pin<Box<dyn core::future::Future<Output = ()>>>;
            type Output = ();

            fn into_future(self) -> Self::IntoFuture {
                let Self {
                    app,
                    webview_id,
                    ipc,
                    ..
                } = self;
                let mut runtime = Some(Runtime::new(ipc, webview_id));
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
                                let _guard = JsLockGuard::acquire(rt.ipc(), rt.webview_id());
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
                                    rt.ipc().send_acquire_lock(rt.webview_id());
                                }
                                return Poll::Pending;
                            }
                        }
                    }
                });

                Box::pin(poll_driver)
            }
        }

        BuildFuture::new(app, self.webview_id, self.ipc)
    }
}

/// Create a blank HTTP response.
pub(crate) fn blank_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .body(vec![])
        .expect("Failed to build blank response")
}

/// Create an error HTTP response.
pub(crate) fn error_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(400)
        .body(vec![])
        .expect("Failed to build error response")
}

/// Create a JavaScript module HTTP response.
pub(crate) fn module_response(content: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript")
        .header("access-control-allow-origin", "*")
        .body(content.as_bytes().to_vec())
        .expect("Failed to build module response")
}

/// Create a not found HTTP response.
pub(crate) fn not_found_response() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(404)
        .body(b"Not Found".to_vec())
        .expect("Failed to build not found response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{DecodedVariant, EncodedData, MessageType};

    fn ipc_message(message_type: MessageType) -> IPCMessage {
        let mut data = EncodedData::default();
        data.push_u8(message_type as u8);
        IPCMessage::new(data.to_bytes())
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
        let (ipc, sender) = WryIPC::new(Arc::new(|_| {}));
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
        let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}));
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
        let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}));
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
            let (_ipc, sender) = WryIPC::new(Arc::new(|_| {}));
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

    #[test]
    fn handler_responds_error_when_evaluate_arrives_after_runtime_drop() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let protocol_handler = app_builder.protocol_handler();
        drop(app_builder);

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
        let response = response
            .borrow_mut()
            .take()
            .expect("closed runtime should receive an error response");
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn lock_request_is_queued_until_webview_loads() {
        let bindgen = WryBindgen::new(|_| {});
        let app_builder = bindgen.app_builder();
        let webview_id = app_builder.webview_id;

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
        let webview_id = app_builder.webview_id;
        let protocol_handler = app_builder.protocol_handler();

        let evaluated_scripts = Rc::new(RefCell::new(Vec::new()));
        let captured_scripts = evaluated_scripts.clone();
        let _prepared_app = app_builder.build(
            || async {},
            move |script| captured_scripts.borrow_mut().push(script.to_string()),
        );

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
            OutboundIPCMessage::new(ipc_message(MessageType::Respond)),
        ));

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

        let response = Rc::new(RefCell::new(None));
        let captured_response = response.clone();
        let request = lock_request();

        let unhandled = protocol_handler.handle_request(
            "wry",
            |_| {},
            &request,
            move |response| *captured_response.borrow_mut() = Some(response),
        );

        assert!(unhandled.is_none());
        let response = response
            .borrow_mut()
            .take()
            .expect("closed runtime should receive an error response");
        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    }
}
