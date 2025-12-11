use std::sync::mpsc::Sender;
use std::sync::RwLock;
use winit::event_loop::EventLoopProxy;
use winit::event_loop::EventLoop;

use crate::encoder::{JSFunction, set_event_loop_proxy, wait_for_js_event};
use crate::ipc::IPCMessage;
use crate::webview::State;

mod encoder;
mod ipc;
mod webview;

pub(crate) struct DomEnv {
    pub(crate) proxy: EventLoopProxy<IPCMessage>,
    pub(crate) queued_rust_calls: RwLock<Vec<IPCMessage>>,
    pub(crate) sender: RwLock<Option<Sender<IPCMessage>>>,
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

const CONSOLE_LOG: JSFunction<fn(String)> = JSFunction::new(0);
const ALERT: JSFunction<fn(String)> = JSFunction::new(1);
const ADD_NUMBERS_JS: JSFunction<fn(i32, i32) -> i32> = JSFunction::new(2);
const ADD_EVENT_LISTENER: JSFunction<fn(String, fn())> = JSFunction::new(3);
const SET_TEXT_CONTENT: JSFunction<fn(String, String)> = JSFunction::new(4);

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
        true
    });
    wait_for_js_event::<()>();
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
                            if (callback.call()) {
                                e.preventDefault();
                                console.log("Event " + event_name + " default prevented by Rust callback.");
                            }
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
