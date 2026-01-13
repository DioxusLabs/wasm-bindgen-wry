use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use wasm_bindgen::{runtime::WryBindgenEvent, wry::WryBindgen};

use crate::home::root_response;

/// Event type for the wry-testing event loop.
/// Wraps wry-bindgen's AppEvent and adds application-level events.
#[derive(Debug)]
pub(crate) enum WryEvent {
    /// An event from wry-bindgen runtime
    App(WryBindgenEvent),
    /// Shutdown the event loop
    Shutdown,
}

// Each platform has a different custom protocol scheme
#[cfg(target_os = "android")]
pub const BASE_URL: &str = "https://wry.index.html";

#[cfg(target_os = "windows")]
pub const BASE_URL: &str = "http://wry.index.html";

#[cfg(not(any(target_os = "android", target_os = "windows")))]
pub const BASE_URL: &str = "wry://index.html";

const PROTOCOL_SCHEME: &str = "wry";

pub(crate) fn run_event_loop<F: Future<Output = ()> + 'static>(
    event_loop: EventLoop<WryEvent>,
    wry_bindgen: WryBindgen,
    app: impl FnOnce() -> F + Send + 'static,
    headless: bool,
) {
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(800, 800))
        .with_visible(!headless)
        .build(&event_loop)
        .unwrap();

    let proxy = event_loop.create_proxy();
    let proxy_clone = proxy.clone();

    let app_builder = wry_bindgen.app_builder();
    let protocol_handler = app_builder.protocol_handler();

    let builder = WebViewBuilder::new()
        .with_devtools(true)
        .with_asynchronous_custom_protocol(PROTOCOL_SCHEME.into(), move |_, request, responder| {
            let responder = |response| responder.respond(response);
            let send_app_event = |event| {
                proxy_clone.send_event(WryEvent::App(event)).unwrap();
            };
            let responder = protocol_handler.handle_request(
                PROTOCOL_SCHEME,
                send_app_event,
                &request,
                responder,
            );
            let Some(responder) = responder else {
                return;
            };

            responder(root_response())
        })
        .with_url(BASE_URL);

    // On Linux, use build_gtk for X11 and Wayland support
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.gtk_window()).unwrap()
    };

    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window).unwrap();

    let evaluate_script = move |script: &str| {
        _ = webview.evaluate_script(script);
    };
    let run_app = app_builder.build(app, evaluate_script);

    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run_app.into_future());
        // Signal the event loop to exit after app completes
        let _ = proxy.send_event(WryEvent::Shutdown);
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                std::process::exit(0);
            }
            Event::UserEvent(wry_event) => match wry_event {
                WryEvent::Shutdown => {
                    *control_flow = ControlFlow::Exit;
                }
                WryEvent::App(app_event) => {
                    wry_bindgen.handle_user_event(app_event);
                }
            },
            _ => {}
        }
    });
}
