use std::sync::mpsc::Sender;
use std::sync::RwLock;
use winit::event_loop::EventLoopProxy;
use winit::event_loop::EventLoop;

use crate::encoder::{JSFunction, set_event_loop_proxy, wait_for_js_event};
use crate::ipc::IPCMessage;
use crate::webview::State;

mod encoder;
mod ipc;
mod webview;
mod home;

pub(crate) struct DomEnv {
    pub(crate) proxy: EventLoopProxy<IPCMessage>,
    pub(crate) queued_rust_calls: RwLock<Vec<IPCMessage>>,
    pub(crate) sender: RwLock<Option<Sender<IPCMessage>>>,
}

impl DomEnv {
    fn new(proxy: EventLoopProxy<IPCMessage>) -> Self {
        Self {
            proxy,
            queued_rust_calls: RwLock::new(Vec::new()),
            sender: RwLock::new(None),
        }
    }

    fn js_response(&self, responder: IPCMessage) {
        let _ = self.proxy.send_event(responder);
    }

    fn queue_rust_call(&self, responder: IPCMessage) {
        if let Some(sender) = self.sender.read().unwrap().as_ref() {
            let _ = sender.send(responder);
        } else {
            self.queued_rust_calls.write().unwrap().push(responder);
        }
    }

    fn set_sender(&self, sender: Sender<IPCMessage>) {
        let mut queued = self.queued_rust_calls.write().unwrap();
        *self.sender.write().unwrap() = Some(sender);
        for call in queued.drain(..) {
            if let Some(sender) = self.sender.read().unwrap().as_ref() {
                let _ = sender.send(call);
            }
        }
    }
}

fn main() -> wry::Result<()> {
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
    set_event_loop_proxy(proxy);
    std::thread::spawn(app);
    let mut state = State::default();
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}

const CONSOLE_LOG: JSFunction<fn(String)> = JSFunction::new(0);
const ALERT: JSFunction<fn(String)> = JSFunction::new(1);
const ADD_NUMBERS_JS: JSFunction<fn(i32, i32) -> i32> = JSFunction::new(2);
const ADD_EVENT_LISTENER: JSFunction<fn(String, fn())> = JSFunction::new(3);
const SET_TEXT_CONTENT: JSFunction<fn(String, String)> = JSFunction::new(4);

fn app() {
    let add_function = ADD_NUMBERS_JS;
    let set_text_content = SET_TEXT_CONTENT;
    let assert_sum_works = move || {
        println!("calling add_function from JS...");
        let sum: i32 = add_function.call(5, 7);
        println!("Sum from JS: {}", sum);
        assert_eq!(sum, 12);
    };
    assert_sum_works();
    println!("Setting up event listener...");
    let add_event_listener: JSFunction<fn(_, _)> = JSFunction::new(3);
    let mut count = 0;
    add_event_listener.call("click".to_string(), move || {
        println!("Button clicked!");
        assert_sum_works();
        count += 1;
        let new_text = format!("Button clicked {} times", count);
        set_text_content.call("click-count".to_string(), new_text);
        true
    });
    wait_for_js_event::<()>();
}
