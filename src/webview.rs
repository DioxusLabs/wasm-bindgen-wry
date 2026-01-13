use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use wasm_bindgen::{
    runtime::AppEvent,
    wry::{ProtocolHandler, WryBindgen},
};

use crate::home::root_response;

/// Event type for the wry-testing event loop.
/// Wraps wry-bindgen's AppEvent and adds application-level events.
#[derive(Debug)]
pub(crate) enum WryEvent {
    /// An event from wry-bindgen runtime
    App(AppEvent),
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

pub(crate) fn run_event_loop(
    event_loop: EventLoop<WryEvent>,
    wry_bindgen: WryBindgen,
    protocol_handler: ProtocolHandler,
    headless: bool,
) {
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(800, 800))
        .with_visible(!headless)
        .build(&event_loop)
        .unwrap();

    let proxy = event_loop.create_proxy();

    let builder = WebViewBuilder::new()
        .with_devtools(true)
        .with_asynchronous_custom_protocol(PROTOCOL_SCHEME.into(), move |_, request, responder| {
            let responder = |response| responder.respond(response);
            let send_app_event = |event| {
                proxy.send_event(WryEvent::App(event)).unwrap();
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

    webview.open_devtools();

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
                    wry_bindgen.handle_user_event(app_event, |script| {
                        if let Err(err) = webview.evaluate_script(script) {
                            eprintln!("Error evaluating script: {err}");
                        }
                    });
                }
            },
            _ => {}
        }
    });
}
