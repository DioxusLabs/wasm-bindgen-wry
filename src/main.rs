use std::fmt::Write;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::sync::mpsc::Sender;
use winit::event_loop::EventLoop;
use winit::event_loop::EventLoopProxy;

use crate::encoder::{JSFunction, JSHeapRef, set_event_loop_proxy, wait_for_js_event};
use crate::ipc::IPCMessage;
use crate::webview::State;

inventory::collect!(JsFunctionSpec);

mod encoder;
mod home;
mod ipc;
mod webview;

pub struct JsFunctionSpec {
    pub name: &'static str,
    pub js_code: &'static str,
    pub type_info: fn() -> (Vec<String>, String),
}

impl JsFunctionSpec {
    pub const fn new(
        name: &'static str,
        js_code: &'static str,
        type_info: fn() -> (Vec<String>, String),
    ) -> Self {
        Self {
            name,
            js_code,
            type_info,
        }
    }
}

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

macro_rules! js_function {
    ($($vis:vis fn $name:ident ($($arg_name:ident : $arg_type:ty),*) -> $ret_type:ty = $js_code:literal)*) => {
        $(
            $vis fn $name($($arg_name : $arg_type),*) -> $ret_type {
                inventory::submit! {
                    JsFunctionSpec::new(
                        stringify!($name),
                        $js_code,
                        || (vec![$(<$arg_type as encoder::TypeConstructor<_>>::create_type_instance()),*], <$ret_type as encoder::TypeConstructor>::create_type_instance())
                    )
                }

                let func: JSFunction<fn($($arg_type),*) -> $ret_type> = {
                    FUNCTION_REGISTRY.get_function(stringify!($name)).expect("Function not found in registry")
                };
                func.call($($arg_name),*)
            }
        )*
    };
}

js_function! {
    pub fn console_log(msg: String) -> () = "(msg) => { console.log(msg); }"
    pub fn alert(msg: String) -> () = "(msg) => { alert(msg); }"
    pub fn add_numbers(a: u32, b: u32) -> u32 = "((a, b) => a + b)"
    pub fn add_event_listener(event: String, callback: Box<dyn FnMut() -> bool>) -> () = "((event, callback) => { window.addEventListener(event, () => { return callback(); }); })"
    pub fn create_element(tag: String) -> JSHeapRef = "(tag) => document.createElement(tag)"
    pub fn append_child(parent: JSHeapRef, child: JSHeapRef) -> () = "((parent, child) => { parent.appendChild(child); })"
    pub fn set_attribute(element: JSHeapRef, attr: String, value: String) -> () = "((element, attr, value) => { element.setAttribute(attr, value); })"
    pub fn set_text(element: JSHeapRef, text: String) -> () = "((element, text) => { element.textContent = text; })"
    pub fn get_body() -> JSHeapRef = "(() => document.body)"
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
    let registry = &*FUNCTION_REGISTRY;

    std::thread::spawn(move || app());
    let mut state = State::new(registry);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}

struct FunctionRegistry {
    functions: String,
    function_ids: Vec<JsFunctionId>,
}

struct JsFunctionId {
    name: &'static str,
}

static FUNCTION_REGISTRY: LazyLock<FunctionRegistry> = LazyLock::new(|| {
    FunctionRegistry::collect_from_inventory()
});

impl FunctionRegistry {
    fn collect_from_inventory() -> Self {
        let mut script = String::from("window.setFunctionRegistry([");
        let mut function_ids = Vec::new();
        for (i, spec) in inventory::iter::<JsFunctionSpec>().enumerate() {
            if i > 0 {
                script.push_str(",\n");
            }
            let (args, return_type) = (spec.type_info)();
            let id = JsFunctionId { name: spec.name };
            function_ids.push(id);
            write!(
                &mut script,
                "window.createWrapperFunction([{}], {}, {})",
                args.join(", "),
                return_type,
                spec.js_code
            )
            .unwrap();
        }
        script.push_str("]);");
        Self {
            functions: script,
            function_ids,
        }
    }

    fn get_function<F>(&self, name: &str) -> Option<JSFunction<F>>
    where
        F: 'static,
    {
        for (i, id) in self.function_ids.iter().enumerate() {
            if id.name == name {
                return Some(JSFunction::new(i as u32));
            }
        }
        None
    }

    fn script(&self) -> &str {
        &self.functions
    }
}

fn app() {
    std::thread::sleep(std::time::Duration::from_secs(1));
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let sum = add_numbers(123u32, 456u32);
        if sum != 579 {
            panic!("Incorrect sum: {}", sum);
        }
    }
    let duration = start.elapsed();
    println!(
        "Performed 100 add_numbers calls in {:?} milliseconds",
        duration.as_millis()
    );
    println!(
        "Average time per call: {:?} milliseconds",
        duration.as_millis() as f64 / 1000.0
    );

    // Get document body
    let body: JSHeapRef = get_body();

    // Create a container div
    let container: JSHeapRef = create_element("div".to_string());
    set_attribute(container, "id".to_string(), "heap-demo".to_string());
    set_attribute(container, "style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

    // Create a heading
    let heading: JSHeapRef = create_element("h2".to_string());
    set_text(heading, "JSHeap Demo".to_string());
    set_attribute(
        heading,
        "style".to_string(),
        "color: #333; margin-top: 0;".to_string(),
    );
    append_child(container, heading);

    // Create info paragraph
    let info: JSHeapRef = create_element("p".to_string());
    set_text(
        info,
        format!("Heap ref ID for this container: {}", container.id()),
    );
    append_child(container, info);
    // Create a counter display
    let counter_display: JSHeapRef = create_element("p".to_string());
    set_attribute(
        counter_display,
        "id".to_string(),
        "heap-counter".to_string(),
    );
    set_attribute(
        counter_display,
        "style".to_string(),
        "font-size: 24px; font-weight: bold; color: #2196F3;".to_string(),
    );
    set_text(counter_display, "Counter: 0".to_string());
    append_child(container, counter_display);

    // Create a button
    let button: JSHeapRef = create_element("button".to_string());
    set_text(button, "Click me (heap-managed)".to_string());
    set_attribute(button, "id".to_string(), "heap-button".to_string());
    set_attribute(button, "style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
    append_child(container, button);
    // Append container to body
    append_child(body, container);

    // Demo 4: Event handling with heap refs
    let mut count = 0;

    // Store the counter display ref for use in the closure
    let counter_ref = counter_display;

    add_event_listener(
        "click".to_string(),
        Box::new(move || {
            count += 1;

            // Update the counter display using the heap ref
            let start = std::time::Instant::now();
            set_text(counter_ref, format!("Counter: {}", count));
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
