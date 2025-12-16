use std::fmt::Write;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::sync::mpsc::Sender;
use winit::event_loop::EventLoop;
use winit::event_loop::EventLoopProxy;

use crate::encoder::{
    BatchState, JSFunction, JSHeapRef, batch, set_event_loop_proxy, wait_for_js_event,
};
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

macro_rules! js_type {
    ($vis:vis type $name:ident;) => {
        #[derive(Clone, Debug)]
        $vis struct $name(JSHeapRef);

        impl encoder::TypeConstructor for $name {
            fn create_type_instance() -> String {
                JSHeapRef::create_type_instance()
            }
        }

        impl encoder::BinaryEncode for $name {
            fn encode(self, encoder: &mut crate::ipc::EncodedData) {
                self.0.encode(encoder);
            }
        }

        impl encoder::BinaryDecode for $name {
            fn decode(decoder: &mut crate::ipc::DecodedData) -> Result<Self, ()> {
                JSHeapRef::decode(decoder).map(Self)
            }
        }

        impl encoder::BatchableResult for $name {
            fn needs_flush() -> bool {
                false
            }

            fn batched_placeholder(batch: &mut BatchState) -> Self {
                Self(JSHeapRef::batched_placeholder(batch))
            }
        }
    };
}

macro_rules! js_function {
    ($vis:vis fn $name:ident ($($arg_name:ident : $arg_type:ty),*) -> $ret_type:ty = $js_code:literal;) => {
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
    };
}

js_type!(
    pub type Element;
);
js_function!(pub fn console_log(msg: String) -> () = "(msg) => { console.log(msg); }";);
js_function!(pub fn alert(msg: String) -> () = "(msg) => { alert(msg); }";);
js_function!(pub fn add_numbers(a: u32, b: u32) -> u32 = "((a, b) => a + b)";);
js_function!(pub fn add_event_listener(event: String, callback: Box<dyn FnMut() -> bool>) -> JSHeapRef = "((event, callback) => { window.addEventListener(event, callback); return callback; })";);
js_function!(pub fn remove_event_listener(event: String, listener: JSHeapRef) -> () = "((event, listener) => { window.removeEventListener(event, listener); })";);
js_function!(pub fn create_element(tag: String) -> Element = "(tag) => document.createElement(tag)";);
js_function!(pub fn append_child(parent: Element, child: Element) -> () = "((parent, child) => { parent.appendChild(child); })";);
js_function!(pub fn set_attribute(element: Element, attr: String, value: String) -> () = "((element, attr, value) => { element.setAttribute(attr, value); })";);
js_function!(pub fn set_text(element: Element, text: String) -> () = "((element, text) => { element.textContent = text; })";);
js_function!(pub fn get_body() -> Element = "(() => document.body)";);

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

    std::thread::spawn(app);
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

static FUNCTION_REGISTRY: LazyLock<FunctionRegistry> =
    LazyLock::new(FunctionRegistry::collect_from_inventory);

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
    batch(|| {
        // Get document body
        let body = get_body();

        // Create a container div
        let container = create_element("div".to_string());
        set_attribute(container.clone(), "id".to_string(), "heap-demo".to_string());
        set_attribute(container.clone(), "style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

        // Create a heading
        let heading = create_element("h2".to_string());
        set_text(heading.clone(), "JSHeap Demo".to_string());
        set_attribute(
            heading.clone(),
            "style".to_string(),
            "color: #333; margin-top: 0;".to_string(),
        );
        append_child(container.clone(), heading);

        // Create a counter display
        let counter_display = create_element("p".to_string());
        set_attribute(
            counter_display.clone(),
            "id".to_string(),
            "heap-counter".to_string(),
        );
        set_attribute(
            counter_display.clone(),
            "style".to_string(),
            "font-size: 24px; font-weight: bold; color: #2196F3;".to_string(),
        );
        set_text(counter_display.clone(), "Counter: 0".to_string());
        append_child(container.clone(), counter_display.clone());

        // Create a button
        let button = create_element("button".to_string());
        set_text(button.clone(), "Click me (heap-managed)".to_string());
        set_attribute(button.clone(), "id".to_string(), "heap-button".to_string());
        set_attribute(button.clone(), "style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
        append_child(container.clone(), button);
        // Append container to body
        append_child(body, container);

        let counter_ref = counter_display.clone();
        // Demo 4: Event handling with heap refs
        let mut count = 0;
        let listener = add_event_listener(
            "click".to_string(),
            Box::new(move || {
                count += 1;

                // Update the counter display using the heap ref
                let start = std::time::Instant::now();
                set_text(counter_ref.clone(), format!("Counter: {}", count));
                let duration = start.elapsed();
                println!(
                    "Updated counter display in {:?} microseconds",
                    duration.as_micros()
                );

                true
            }),
        );
    });
    
    // Keep running to handle events
    wait_for_js_event::<()>();
}
