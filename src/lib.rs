//! Wry-bindgen webview library
//!
//! This library provides the infrastructure for launching a webview with
//! Rust-JavaScript bindings via the wry-bindgen macro system.

use winit::event_loop::EventLoop;

use wasm_bindgen::{Closure, start_app};

pub mod bindings;
mod home;
mod webview;

use webview::State;

// Re-export bindings for convenience
pub use bindings::set_on_log;

// Re-export prelude items that apps need
pub use wasm_bindgen::JsValue;
pub use wasm_bindgen::prelude::batch;
pub use wasm_bindgen::run_on_main_thread;

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
            let error = error as *mut x11_dl::xlib::XErrorEvent;
            (unsafe { (*error).error_code }) == 170
        }));
    }

    let app = || async move {
        set_on_error(Closure::new(|err: String, stack: String| {
            println!("[ERROR IN JS CONSOLE] {err}\nStack trace:\n{stack}");
        }));

        set_on_log(Closure::new(|msg: String| {
            println!("[JS] {msg}");
        }));
        app().await
    };

    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();

    let event_loop_proxy = {
        let proxy = proxy.clone();
        move |event| {
            _ = proxy.send_event(event);
        }
    };

    let wry_bindgen = start_app(event_loop_proxy, app, |future| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future);
    })
    .unwrap();

    let mut state = State::new(wry_bindgen, proxy, headless);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}
