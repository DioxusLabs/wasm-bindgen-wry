use std::fmt::{Display, Write};
use std::sync::RwLock;
use std::sync::mpsc::Sender;
use winit::event_loop::EventLoop;
use winit::event_loop::EventLoopProxy;

use crate::encoder::WrapJsFunction;
use crate::encoder::{JSFunction, JSHeapRef, set_event_loop_proxy, wait_for_js_event};
use crate::ipc::IPCMessage;
use crate::webview::State;

mod encoder;
mod home;
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
    let mut registry = FunctionRegistry::new();
    let console_log: JSFunction<fn(String)> = registry.push("(msg) => { console.log(msg); }");
    let alert: JSFunction<fn(String) -> ()> = registry.push("(msg) => { alert(msg); }");
    let add_numbers: JSFunction<fn(u32, u32) -> u32> = registry.push("((a, b) => a + b)");
    let add_event_listener: JSFunction<fn(String, Box<dyn FnMut() -> bool>) -> ()> =
        registry.push("((event, callback) => { window.addEventListener(event, () => { return callback(); }); })");
    let create_element: JSFunction<fn(String) -> JSHeapRef> = registry.push("(tag) => document.createElement(tag)");
    let append_child: JSFunction<fn(JSHeapRef, JSHeapRef) -> ()> = registry.push("((parent, child) => { parent.appendChild(child); })");
    let set_attribute: JSFunction<fn(JSHeapRef, String, String) -> ()> = registry.push("((element, attr, value) => { element.setAttribute(attr, value); })");
    let set_text: JSFunction<fn(JSHeapRef, String) -> ()> = registry.push("((element, text) => { element.textContent = text; })");
    let get_body: JSFunction<fn() -> JSHeapRef> = registry.push("(() => document.body)");
    std::thread::spawn(move || app(
        console_log,
        add_numbers,
        add_event_listener,
        create_element,
        append_child,
        set_attribute,
        set_text,
        get_body,
    ));
    let mut state = State::new(registry);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}

struct FunctionRegistry {
    functions: String,
    function_count: u32,
}

impl FunctionRegistry {
    fn new() -> Self {
        Self {
            functions: String::from("window.setFunctionRegistry(["),
            function_count: 0,
        }
    }

    fn push<F: WrapJsFunction<P>, P>(&mut self, f: impl Display) -> JSFunction<F> {
        if self.function_count > 0 {
            self.functions.push_str(",\n");
        }
        F::wrap_js_function_with_encoder_decoder(&mut self.functions);
        write!(&mut self.functions, "({})", f).unwrap();

        let f = JSFunction::new(self.function_count);
        self.function_count += 1;
        f
    }

    fn build_registry_script(&self) -> String {
        let mut script = self.functions.clone();
        script.push_str("]);");
        println!("Function registry script:\n{}", script);
        script
    }
}

fn app(
    console_log: JSFunction<fn(String)>,
    add_numbers: JSFunction<fn(u32, u32) -> u32>,
    add_event_listener: JSFunction<fn(String, Box<dyn FnMut() -> bool>) -> ()>,
    create_element: JSFunction<fn(String) -> JSHeapRef>,
    append_child: JSFunction<fn(JSHeapRef, JSHeapRef) -> ()>,
    set_attribute: JSFunction<fn(JSHeapRef, String, String) -> ()>,
    set_text: JSFunction<fn(JSHeapRef, String) -> ()>,
    get_body: JSFunction<fn() -> JSHeapRef>,
) {
    // Get document body
    let body: JSHeapRef = get_body.call(());

    // Create a container div
    let container: JSHeapRef = create_element.call("div".to_string());
    set_attribute.call(container, "id".to_string(), "heap-demo".to_string());
    set_attribute.call(container, "style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

    // Create a heading
    let heading: JSHeapRef = create_element.call("h2".to_string());
    set_text.call(heading, "JSHeap Demo".to_string());
    set_attribute.call(
        heading,
        "style".to_string(),
        "color: #333; margin-top: 0;".to_string(),
    );
    append_child.call(container, heading);

    // Create info paragraph
    let info: JSHeapRef = create_element.call("p".to_string());
    set_text.call(
        info,
        format!("Heap ref ID for this container: {}", container.id()),
    );
    append_child.call(container, info);
    // Create a counter display
    let counter_display: JSHeapRef = create_element.call("p".to_string());
    set_attribute.call(
        counter_display,
        "id".to_string(),
        "heap-counter".to_string(),
    );
    set_attribute.call(
        counter_display,
        "style".to_string(),
        "font-size: 24px; font-weight: bold; color: #2196F3;".to_string(),
    );
    set_text.call(counter_display, "Counter: 0".to_string());
    append_child.call(container, counter_display);

    // Create a button
    let button: JSHeapRef = create_element.call("button".to_string());
    set_text.call(button, "Click me (heap-managed)".to_string());
    set_attribute.call(button, "id".to_string(), "heap-button".to_string());
    set_attribute.call(button, "style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
    append_child.call(container, button);
    // Append container to body
    append_child.call(body, container);

    // Demo 4: Event handling with heap refs
    let mut count = 0;

    // Store the counter display ref for use in the closure
    let counter_ref = counter_display;

    add_event_listener.call(
        "click".to_string(),
        Box::new(move || {
            count += 1;

            // Update the counter display using the heap ref
            let start = std::time::Instant::now();
            set_text.call(counter_ref, format!("Counter: {}", count));
            let duration = start.elapsed();
            println!(
                "Updated counter display in {:?} microseconds",
                duration.as_micros()
            );

            true
        }),
    );

    // Keep running to handle events
    wait_for_js_event::<()>();
}
