use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use wasm_bindgen::{runtime::AppEvent, wry::WryBindgen};

use crate::home::root_response;

pub(crate) fn run_event_loop(
    event_loop: EventLoop<AppEvent>,
    wry_bindgen: WryBindgen,
    headless: bool,
) {
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(800, 800))
        .with_visible(!headless)
        .build(&event_loop)
        .unwrap();

    let proxy = event_loop.create_proxy();
    let protocol_handler = wry_bindgen.create_protocol_handler(move |event| {
        proxy.send_event(event).unwrap();
    });

    let builder = WebViewBuilder::new()
        .with_devtools(true)
        .with_asynchronous_custom_protocol("wry".into(), move |_, request, responder| {
            let responder = |response| responder.respond(response);
            let Some(responder) = protocol_handler(&request, responder) else {
                return;
            };

            responder(root_response())
        })
        .with_url("wry://index");

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
            Event::UserEvent(app_event) => {
                if let Some(status) = wry_bindgen.handle_user_event(app_event, |script| {
                    if let Err(err) = webview.evaluate_script(script) {
                        eprintln!("Error evaluating script: {err}");
                    }
                }) {
                    std::process::exit(status);
                }
            }
            _ => {}
        }
    });
}
