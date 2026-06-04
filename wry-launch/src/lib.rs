//! Wry-bindgen webview library
//!
//! This library provides the infrastructure for launching a webview with
//! Rust-JavaScript bindings via the wry-bindgen macro system.

use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopBuilder;

use wasm_bindgen::Closure;
use wry_bindgen_runtime::WryBindgen;

pub mod bindings;
mod home;
mod webview;

use webview::{WryEvent, run_event_loop};

// Re-export bindings for convenience
pub use bindings::{set_on_error, set_on_log};

// Re-export prelude items that apps need
pub use wasm_bindgen::JsValue;
pub use wry_bindgen_runtime::wire::batch;

// Re-export tao and wry for users to configure builders
pub use tao;
pub use tao::window::WindowBuilder;
pub use wry;
pub use wry::WebViewBuilder;

/// Builder for launching a wry-launch application with custom window and webview settings.
///
/// # Example
///
/// ```ignore
/// use wry_launch::{LaunchBuilder, WindowBuilder, WebViewBuilder};
/// use wry_launch::tao::dpi::LogicalSize;
///
/// fn main() -> wry::Result<()> {
///     let window = WindowBuilder::new()
///         .with_title("My App")
///         .with_inner_size(LogicalSize::new(1024.0, 768.0));
///
///     let webview = WebViewBuilder::new()
///         .with_devtools(false);
///
///     LaunchBuilder::new()
///         .window(window)
///         .webview(webview)
///         .run(|| async {
///             // Your app code here
///         })
/// }
/// ```
pub struct LaunchBuilder {
    window: WindowBuilder,
    webview: WebViewBuilder<'static>,
}

impl Default for LaunchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchBuilder {
    /// Create a new launch builder with default settings.
    pub fn new() -> Self {
        Self {
            window: WindowBuilder::new()
                .with_title("wry-launch")
                .with_inner_size(LogicalSize::new(800.0, 600.0)),
            webview: WebViewBuilder::new().with_devtools(true),
        }
    }

    /// Set the window builder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use wry_launch::{LaunchBuilder, WindowBuilder};
    ///
    /// let window = WindowBuilder::new()
    ///     .with_title("My App")
    ///     .with_inner_size(LogicalSize::new(1024.0, 768.0));
    ///
    /// LaunchBuilder::new().window(window)
    /// ```
    pub fn window(mut self, window: WindowBuilder) -> Self {
        self.window = window;
        self
    }

    /// Set the webview builder.
    ///
    /// Note: The custom protocol and URL are set automatically and should not be overridden.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use wry_launch::{LaunchBuilder, WebViewBuilder};
    ///
    /// let webview = WebViewBuilder::new()
    ///     .with_devtools(true)
    ///     .with_transparent(false);
    ///
    /// LaunchBuilder::new().webview(webview)
    /// ```
    pub fn webview(mut self, webview: WebViewBuilder<'static>) -> Self {
        self.webview = webview;
        self
    }

    /// Run the application with the configured settings.
    pub fn run<F, Fut>(self, app: F) -> wry::Result<()>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()>,
    {
        let app = || async move {
            set_on_error(Closure::new(|err: String, stack: String| {
                println!("[ERROR IN JS CONSOLE] {err}\nStack trace:\n{stack}");
            }));

            set_on_log(Closure::new(|msg: String| {
                println!("[JS] {msg}");
            }));
            app().await
        };

        let event_loop = EventLoopBuilder::<WryEvent>::with_user_event().build();
        let wry_bindgen = WryBindgen::new();

        run_event_loop(event_loop, wry_bindgen, app, self.window, self.webview);

        Ok(())
    }
}

/// Run a webview application with the given app function.
///
/// The app function will be spawned in a separate thread and can use
/// the wry-bindgen bindings to interact with the JavaScript runtime.
///
/// # Example
///
/// ```ignore
/// use wry_launch::{run, batch, WINDOW};
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
    LaunchBuilder::new().run(app)
}

/// Run a headless webview application with the given app function.
///
/// This is identical to `run()` except the window will be invisible.
/// Useful for testing, automation, or background processing.
///
/// # Example
///
/// ```ignore
/// use wry_launch::{run_headless, batch, WINDOW};
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
    let window = WindowBuilder::new()
        .with_title("wry-launch")
        .with_inner_size(LogicalSize::new(800.0, 600.0))
        .with_visible(false);

    let webview = WebViewBuilder::new()
        .with_devtools(true)
        .with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled);

    LaunchBuilder::new()
        .window(window)
        .webview(webview)
        .run(app)
}
