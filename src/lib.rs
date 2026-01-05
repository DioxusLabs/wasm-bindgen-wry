//! Wry-bindgen webview library
//!
//! This library provides the infrastructure for launching a webview with
//! Rust-JavaScript bindings via the wry-bindgen macro system.

use futures_util::FutureExt;
use wasm_bindgen::runtime::progress_js_with;
use winit::event_loop::EventLoop;

use wasm_bindgen::{Closure, FUNCTION_REGISTRY, FunctionRegistry};

pub mod bindings;
mod home;
mod webview;
pub mod wry_bindgen;

use webview::State;

// Re-export bindings for convenience
pub use bindings::set_on_log;
pub use wry_bindgen::WryBindgen;

// Re-export prelude items that apps need
pub use wasm_bindgen::JsValue;
pub use wasm_bindgen::prelude::{
    AppEvent, batch, set_event_loop_proxy, shutdown, wait_for_js_result,
};

use crate::bindings::set_on_error;

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
pub fn run<F, Fut>(app: F) -> wry::Result<()>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
{
    run_with_config(app, false)
}

/// Run a headless webview application with the given app function.
///
/// This is identical to `run()` except the window will be invisible.
/// Useful for testing, automation, or background processing.
///
/// # Example
///
/// ```ignore
/// use wry_testing::{run_headless, batch, WINDOW};
///
/// fn main() -> wry::Result<()> {
///     run_headless(app)
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
pub fn run_headless<F, Fut>(app: F) -> wry::Result<()>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
{
    run_with_config(app, true)
}

fn run_with_config<F, Fut>(app: F, headless: bool) -> wry::Result<()>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
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

    let app = || async move {
        set_on_error(Closure::new(|err: String, stack: String| {
            println!("[ERROR IN JS CONSOLE] {}\nStack trace:\n{}", err, stack);
        }));

        set_on_log(Closure::new(|msg: String| {
            println!("[JS] {}", msg);
        }));
        app().await
    };

    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    set_event_loop_proxy({
        let proxy = proxy.clone();
        Box::new(move |event| {
            proxy.send_event(event).unwrap();
        })
    });
    let registry = &*FUNCTION_REGISTRY;

    // Spawn the app thread with panic handling - if the app panics, shut down the webview
    std::thread::spawn(move || {
        let run = || {
            let run_app = app();
            let wait_for_events = async move {
                loop {
                    progress_js_with(|_| unreachable!("no response expected from root task")).await;
                }
            };

            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                futures_util::select! {
                    _ = run_app.fuse() => {},
                    _ = wait_for_events.fuse() => {},
                }
            });
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
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

    let mut state = State::new(registry, proxy, headless);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}
