//! DOM bindings using the wasm_bindgen macro
//!
//! This module provides DOM API bindings using the wry_bindgen macro,
//! which generates code compatible with the IPC protocol.

use wasm_bindgen_macro::wasm_bindgen;

#[wasm_bindgen]
extern "C" {
    /// A DOM Element type
    #[derive(Clone)]
    pub type Element;

    /// Log a message to the console
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn console_log(msg: String);

    /// Show an alert dialog
    #[wasm_bindgen(js_name = alert)]
    pub fn alert(msg: String);

    /// Append a child element to a parent
    #[wasm_bindgen(method, js_class = "Element", js_name = appendChild)]
    pub fn append_child(this: &Element, child: Element);

    /// Set an attribute on an element
    #[wasm_bindgen(method, js_class = "Element", js_name = setAttribute)]
    pub fn set_attribute(this: &Element, attr: String, value: String);

    /// Set the text content of an element
    #[wasm_bindgen(method, setter, js_class = "Element", js_name = textContent)]
    pub fn set_text_content(this: &Element, text: String);

    /// Add an event listener to an element
    #[wasm_bindgen(method, js_class = "Element", js_name = addEventListener)]
    pub fn add_event_listener(this: &Element, event: String, listener: Box<dyn FnMut()>);

}

#[wasm_bindgen(inline_js = "export function add(a, b) { return a + b; }")]
extern "C" {
    /// Add two numbers
    #[wasm_bindgen(js_name = add)]
    pub fn add_numbers(a: u32, b: u32) -> u32;
}

#[wasm_bindgen]
extern "C" {
    /// The window type
    pub type Window;

    /// The global window object (lazily initialized)
    #[wasm_bindgen(thread_local_v2, js_name = window)]
    pub static WINDOW: Window;

    /// Get the document
    #[wasm_bindgen(method, getter, js_class = "Window", js_name = document)]
    pub fn document(this: &Window) -> Document;
}

#[wasm_bindgen]
extern "C" {
    /// The document type
    pub type Document;

    /// Get the body element
    #[wasm_bindgen(method, getter, js_class = "Document", js_name = body)]
    pub fn body(this: &Document) -> Element;

    /// Create a new element with the given tag name
    #[wasm_bindgen(method, js_class = "Document", js_name = createElement)]
    pub fn create_element(this: &Document, tag: String) -> Element;
}

#[wasm_bindgen(inline_js = r#"
const originalLog = console.log;
const originalWarn = console.warn;
const originalError = console.error;

let onLogCallback = null;

function formatArgs(args) {
    return Array.from(args).map(arg => {
        try {
            return typeof arg === 'object' ? JSON.stringify(arg) : String(arg);
        } catch (e) {
            return String(arg);
        }
    }).join(' ');
}

console.log = function(...args) {
    originalLog.apply(console, args);
    onLogCallback && onLogCallback(formatArgs(args));
};

console.warn = function(...args) {
    originalWarn.apply(console, args);
    onLogCallback && onLogCallback('WARN: ' + formatArgs(args));
};

console.error = function(...args) {
    originalError.apply(console, args);
    onLogCallback && onLogCallback('ERROR: ' + formatArgs(args));
};

export function set_on_log(callback) {
    originalLog.call(console, "Setting onLogCallback");
    onLogCallback = callback;
}

export function set_on_error(callback) {
    window.addEventListener('error', function(event) {
        callback(event.message + ' at ' + event.filename + ':' + event.lineno + ':' + event.colno);
    });
}
"#)]
extern "C" {
    pub fn set_on_log(callback: Box<dyn FnMut(String)>);
    pub fn set_on_error(callback: Box<dyn FnMut(String)>);
}

