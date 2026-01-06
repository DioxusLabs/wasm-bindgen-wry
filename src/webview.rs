use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowId},
};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebViewBuilder};

use wasm_bindgen::runtime::AppEvent;

use crate::FunctionRegistry;
use crate::home::root_response;
use crate::wry_bindgen::WryBindgen;

pub(crate) struct State {
    wry_bindgen: WryBindgen,
    window: Option<Window>,
    webview: Option<wry::WebView>,
    proxy: EventLoopProxy<AppEvent>,
    headless: bool,
}

impl State {
    pub fn new(
        function_registry: &'static FunctionRegistry,
        proxy: EventLoopProxy<AppEvent>,
        headless: bool,
    ) -> Self {
        Self {
            wry_bindgen: WryBindgen::new(function_registry),
            window: None,
            webview: None,
            proxy,
            headless,
        }
    }
}

impl ApplicationHandler<AppEvent> for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = Window::default_attributes();
        attributes.inner_size = Some(LogicalSize::new(800, 800).into());
        attributes.visible = !self.headless;
        let window = event_loop.create_window(attributes).unwrap();

        let proxy = self.proxy.clone();
        let protocol_handler = self.wry_bindgen.create_protocol_handler(
            move |event| {
                proxy.send_event(event).unwrap();
            },
            root_response,
        );

        let webview = WebViewBuilder::new()
            .with_devtools(true)
            .with_asynchronous_custom_protocol("wry".into(), move |_, request, responder| {
                protocol_handler(&request, responder);
            })
            .with_url("wry://index")
            .build_as_child(&window)
            .unwrap();

        webview.open_devtools();
        let script = self.wry_bindgen.init_script();
        webview.evaluate_script(script).unwrap();

        self.window = Some(window);
        self.webview = Some(webview);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let window = self.window.as_ref().unwrap();
                let webview = self.webview.as_ref().unwrap();

                let size = size.to_logical::<u32>(window.scale_factor());
                webview
                    .set_bounds(Rect {
                        position: LogicalPosition::new(0, 0).into(),
                        size: LogicalSize::new(size.width, size.height).into(),
                    })
                    .unwrap();
            }
            WindowEvent::CloseRequested => {
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        if let Some(webview) = &self.webview {
            if let Some(status) = self.wry_bindgen.handle_user_event(event, webview) {
                event_loop.exit();
                std::process::exit(status);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        ))]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }
    }
}
