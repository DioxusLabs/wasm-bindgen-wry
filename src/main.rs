use std::sync::mpsc::Sender;
use std::sync::RwLock;
use winit::event_loop::EventLoopProxy;
use winit::event_loop::EventLoop;

use crate::encoder::{JSFunction, JSHeapRef, set_event_loop_proxy, wait_for_js_event};
use crate::ipc::IPCMessage;
use crate::webview::State;

mod encoder;
mod ipc;
mod webview;
mod home;

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

/// JS Function definitions with binary serialization instructions:
/// 
/// Each function has a unique ID and specifies how arguments are serialized/deserialized.
/// The binary format is NOT self-describing, so each side must know the schema.
/// 
/// Format: Arguments are encoded in order using push_* methods:
/// - String: push_str (length as u32, then UTF-8 bytes in str buffer)
/// - u32/i32: push_u32
/// - bool: push_u8 (0 or 1)
/// - JSHeapRef: push_u32 (the reference ID)
/// - fn(): push_u32 (callback ID registered with FunctionEncoder)

/// console.log(message: String) -> ()
/// Serialize: push_str(message)
/// Deserialize return: nothing
#[allow(dead_code)]
const CONSOLE_LOG: JSFunction<fn(String) -> ()> = JSFunction::new(0);

/// alert(message: String) -> ()
/// Serialize: push_str(message)
/// Deserialize return: nothing
#[allow(dead_code)]
const ALERT: JSFunction<fn(String) -> ()> = JSFunction::new(1);

/// add_numbers(a: i32, b: i32) -> i32
/// Serialize: push_u32(a), push_u32(b)
/// Deserialize return: take_u32() as i32
#[allow(dead_code)]
const ADD_NUMBERS_JS: JSFunction<fn(i32, i32) -> i32> = JSFunction::new(2);

/// add_event_listener(event_name: String, callback: Callback) -> ()
/// Serialize: push_str(event_name), push_u32(callback_id)
/// The callback returns bool: take_u8() != 0
const ADD_EVENT_LISTENER: JSFunction<fn(String, Box<dyn FnMut() -> bool>) -> ()> =
    JSFunction::new(3);

/// set_text_content(element_id: String, text: String) -> ()
/// Serialize: push_str(element_id), push_str(text)
/// Deserialize return: nothing
const SET_TEXT_CONTENT: JSFunction<fn(String, String) -> ()> = JSFunction::new(4);

/// heap_has(id: u32) -> bool
/// Serialize: push_u32(id)
/// Deserialize return: take_u8() != 0
const HEAP_HAS: JSFunction<fn(u32) -> bool> = JSFunction::new(8);

/// get_body() -> JSHeapRef
/// Serialize: nothing
/// Deserialize return: take_u32() as JSHeapRef
const GET_BODY: JSFunction<fn() -> JSHeapRef> = JSFunction::new(13);

/// query_selector(selector: String) -> Option<JSHeapRef>
/// Serialize: push_str(selector)
/// Deserialize return: take_u8() for has_value, then take_u32() if has_value
#[allow(dead_code)]
const QUERY_SELECTOR: JSFunction<fn(String) -> Option<JSHeapRef>> = JSFunction::new(14);

/// create_element(tag: String) -> JSHeapRef
/// Serialize: push_str(tag)
/// Deserialize return: take_u32() as JSHeapRef
const CREATE_ELEMENT: JSFunction<fn(String) -> JSHeapRef> = JSFunction::new(15);

/// append_child(parent: JSHeapRef, child: JSHeapRef) -> ()
/// Serialize: push_u32(parent.id), push_u32(child.id)
/// Deserialize return: nothing
const APPEND_CHILD: JSFunction<fn(JSHeapRef, JSHeapRef) -> ()> = JSFunction::new(16);

/// set_attribute(element: JSHeapRef, name: String, value: String) -> ()
/// Serialize: push_u32(element.id), push_str(name), push_str(value)
/// Deserialize return: nothing
const SET_ATTRIBUTE: JSFunction<fn(JSHeapRef, String, String) -> ()> = JSFunction::new(17);

/// set_text(element: JSHeapRef, text: String) -> ()
/// Serialize: push_u32(element.id), push_str(text)
/// Deserialize return: nothing
const SET_TEXT: JSFunction<fn(JSHeapRef, String) -> ()> = JSFunction::new(18);

fn app() {
    println!("=== JSHeap Demo ===\n");


    // Demo 3: DOM manipulation using heap refs
    println!("\n3. Creating DOM elements using heap refs...");

    // Get document body
    let body: JSHeapRef = GET_BODY.call(());
    println!("   Got body element (heap id: {})", body.id());

    // Create a container div
    let container: JSHeapRef = CREATE_ELEMENT.call("div".to_string());
    SET_ATTRIBUTE.call(container, "id".to_string(), "heap-demo".to_string());
    SET_ATTRIBUTE.call(container, "style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

    // Create a heading
    let heading: JSHeapRef = CREATE_ELEMENT.call("h2".to_string());
    SET_TEXT.call(heading, "JSHeap Demo".to_string());
    SET_ATTRIBUTE.call(heading, "style".to_string(), "color: #333; margin-top: 0;".to_string());
    APPEND_CHILD.call(container, heading);

    // Create info paragraph
    let info: JSHeapRef = CREATE_ELEMENT.call("p".to_string());
    SET_TEXT.call(info, format!("Heap ref ID for this container: {}", container.id()));
    APPEND_CHILD.call(container, info);

    // Create a counter display
    let counter_display: JSHeapRef = CREATE_ELEMENT.call("p".to_string());
    SET_ATTRIBUTE.call(counter_display, "id".to_string(), "heap-counter".to_string());
    SET_ATTRIBUTE.call(counter_display, "style".to_string(),
        "font-size: 24px; font-weight: bold; color: #2196F3;".to_string());
    SET_TEXT.call(counter_display, "Counter: 0".to_string());
    APPEND_CHILD.call(container, counter_display);

    // Create a button
    let button: JSHeapRef = CREATE_ELEMENT.call("button".to_string());
    SET_TEXT.call(button, "Click me (heap-managed)".to_string());
    SET_ATTRIBUTE.call(button, "id".to_string(), "heap-button".to_string());
    SET_ATTRIBUTE.call(button, "style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
    APPEND_CHILD.call(container, button);

    // Append container to body
    APPEND_CHILD.call(body, container);
    println!("   Created demo UI with heap-managed elements");

    // Demo 4: Event handling with heap refs
    println!("\n4. Setting up click handler that updates heap-managed element...");
    let mut count = 0;

    // Store the counter display ref for use in the closure
    let counter_ref = counter_display;

    ADD_EVENT_LISTENER.call("click".to_string(), Box::new(move || {
        count += 1;
        println!("   Button clicked! Count: {}", count);

        // Update the counter display using the heap ref
        SET_TEXT.call(counter_ref, format!("Counter: {}", count));

        // Also update the original click-count element
        SET_TEXT_CONTENT.call("click-count".to_string(), format!("Total clicks: {}", count));

        true
    }));

    println!("\n=== Demo ready! Click the button to interact ===\n");

    // Keep running to handle events
    wait_for_js_event::<()>();
}
