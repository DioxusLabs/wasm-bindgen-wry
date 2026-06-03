use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopProxy},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use std::{
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

use wasm_bindgen::wry::{WryBindgen, WryBindgenWebviewDriver};

use crate::home::root_response;

/// Event type for the wry-launch event loop.
#[derive(Debug)]
pub(crate) enum WryEvent {
    /// Poll the wry-bindgen driver on the main thread.
    DriverWake,
    /// Shutdown the event loop
    Shutdown,
}

// Each platform has a different custom protocol scheme
#[cfg(target_os = "android")]
const BASE_URL: &str = "https://wry.index.html";

#[cfg(target_os = "windows")]
const BASE_URL: &str = "http://wry.index.html";

#[cfg(not(any(target_os = "android", target_os = "windows")))]
const BASE_URL: &str = "wry://index.html";

const PROTOCOL_SCHEME: &str = "wry";

struct DriverWake {
    proxy: EventLoopProxy<WryEvent>,
}

impl Wake for DriverWake {
    fn wake(self: Arc<Self>) {
        let _ = self.proxy.send_event(WryEvent::DriverWake);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.proxy.send_event(WryEvent::DriverWake);
    }
}

fn poll_driver(driver: &mut WryBindgenWebviewDriver, waker: &Waker, driver_done: &mut bool) {
    if *driver_done {
        return;
    }

    let mut cx = Context::from_waker(waker);
    if matches!(driver.poll(&mut cx), Poll::Ready(())) {
        *driver_done = true;
    }
}

pub(crate) fn run_event_loop<F>(
    event_loop: EventLoop<WryEvent>,
    wry_bindgen: WryBindgen,
    app: impl FnOnce() -> F + Send + 'static,
    window_builder: WindowBuilder,
    webview_builder: WebViewBuilder<'static>,
) where
    F: Future<Output = ()> + 'static,
{
    let window = window_builder.build(&event_loop).unwrap();

    let proxy = event_loop.create_proxy();
    let proxy_clone = proxy.clone();

    let protocol_handler = wry_bindgen.protocol_handler();

    // Add the required protocol handler and URL to the user-provided webview builder
    let builder = webview_builder
        .with_asynchronous_custom_protocol(PROTOCOL_SCHEME.into(), move |_, request, responder| {
            let responder = |response| responder.respond(response);
            let responder = protocol_handler.handle_request(PROTOCOL_SCHEME, &request, responder);
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

    let (runtime, driver) = wry_bindgen.split();
    let mut driver = driver.with_evaluate_script(move |script| {
        _ = webview.evaluate_script(script);
    });
    let run_app = runtime.run(app);

    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run_app.into_future());
        // Signal the event loop to exit after app completes
        let _ = proxy.send_event(WryEvent::Shutdown);
    });

    let driver_waker = Waker::from(Arc::new(DriverWake { proxy: proxy_clone }));
    let mut driver_done = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(_) => {
                poll_driver(&mut driver, &driver_waker, &mut driver_done);
                if driver_done {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                std::process::exit(0);
            }
            Event::UserEvent(wry_event) => match wry_event {
                WryEvent::DriverWake => {
                    poll_driver(&mut driver, &driver_waker, &mut driver_done);
                    if driver_done {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                WryEvent::Shutdown => {
                    *control_flow = ControlFlow::Exit;
                }
            },
            _ => {}
        }
    });
}
