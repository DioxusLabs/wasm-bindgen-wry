# wry-bindgen-runtime

`wry-bindgen-runtime` is the [`wry`](https://github.com/tauri-apps/wry) transport for [`wry-bindgen`](../wry-bindgen). Pull it into an existing `wry` application to run wry-bindgen's generated JavaScript in a webview while your Rust runs natively.

If you just want a window with everything wired up for you, use [`wry-launch`](../wry-launch) instead, which is built on this crate. Use `wry-bindgen-runtime` directly when you own the window and event loop and want to attach wry-bindgen to a webview yourself.

```rust, no_run
use std::sync::Arc;
use std::task::{Context, Wake, Waker};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;
use wry_bindgen_runtime::WryBindgen;

fn main() -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<PollDriver>::with_user_event().build();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    // One wry-bindgen session per webview.
    let session = WryBindgen::new();
    let handler = session.protocol_handler();

    let webview = WebViewBuilder::new()
        .with_asynchronous_custom_protocol("wry".into(), move |_, request, responder| {
            let respond = |response| responder.respond(response);
            // wry-bindgen answers its own requests; if the handler gives the
            // responder back, the request was yours to serve.
            if let Some(respond) = handler.handle_request("wry", &request, respond) {
                respond(http::Response::new(b"<!doctype html><html></html>".to_vec()));
            }
        })
        .with_url("wry://index.html")
        .build(&window)?;

    // Split the session: the runtime drives your async app on another thread,
    // the driver stays on the webview thread to evaluate scripts.
    let (runtime, driver) = session.split();
    let mut driver = driver.with_evaluate_script(move |js| {
        let _ = webview.evaluate_script(js);
    });

    // Your app is ordinary wasm-bindgen / web-sys code. wry-bindgen carries
    // each call to the webview and brings the result back.
    let run_app = runtime.run(|| async {
        let document = web_sys::window().unwrap().document().unwrap();
        document
            .body()
            .unwrap()
            .set_inner_html("<h1>Hello from native Rust</h1>");
        std::future::pending::<()>().await;
    });
    std::thread::spawn(move || futures::executor::block_on(run_app.into_future()));

    // The driver's waker asks the event loop to poll it again.
    let proxy = event_loop.create_proxy();
    let waker = Waker::from(Arc::new(DriverWaker { proxy }));
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(_) | Event::UserEvent(PollDriver) => {
                if driver.poll(&mut Context::from_waker(&waker)).is_ready() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
    Ok(())
}

struct PollDriver;

struct DriverWaker {
    proxy: EventLoopProxy<PollDriver>,
}

impl Wake for DriverWaker {
    fn wake(self: Arc<Self>) {
        let _ = self.proxy.send_event(PollDriver);
    }
}
```

See [`wry-launch`](../wry-launch) for this same integration packaged up, including per-platform protocol schemes and a headless mode.
