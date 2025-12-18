//! Wry-bindgen webview library
//!
//! This library provides the infrastructure for launching a webview with
//! Rust-JavaScript bindings via the wry-bindgen macro system.

use winit::event_loop::EventLoop;

use wasm_bindgen::{FUNCTION_REGISTRY, FunctionRegistry};

pub mod bindings;
mod home;
mod webview;

use webview::State;

// Re-export bindings for convenience
pub use bindings::set_on_log;
pub use bindings::{Document, Element, WINDOW, Window, alert, console_log};

// Re-export prelude items that apps need
pub use wasm_bindgen::JsValue;
pub use wasm_bindgen::prelude::{
    AppEvent, batch, set_event_loop_proxy, shutdown, wait_for_js_result,
};

/// Run a webview application with the given app function.
///
/// The app function will be spawned in a separate thread and can use
/// the wry-bindgen bindings to interact with the JavaScript runtime.
///
/// # Example
///
/// ```ignore
/// use wry_testing::{run, batch, WINDOW};
///
/// fn main() -> wry::Result<()> {
///     run(app)
/// }
///
/// fn app() {
///     batch(|| {
///         let document = WINDOW.with(|w| w.document());
///         // ... build your UI
///     });
///     wait_for_js_event::<()>();
/// }
/// ```
pub fn run<F>(app: F) -> wry::Result<()>
where
    F: FnOnce() + Send + 'static,
{
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
    set_event_loop_proxy(proxy.clone());
    let registry = &*FUNCTION_REGISTRY;

    // Spawn the app thread with panic handling - if the app panics, shut down the webview
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(app));
        let status = if let Err(panic_info) = result {
            eprintln!("App thread panicked, shutting down webview");
            // Try to print panic info
            if let Some(s) = panic_info.downcast_ref::<&str>() {
                eprintln!("Panic message: {}", s);
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                eprintln!("Panic message: {}", s);
            }
            1 // Exit with error status on panic
        } else {
            0 // Exit with success status on normal completion
        };
        shutdown(status);
    });

    let mut state = State::new(registry, proxy);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}
