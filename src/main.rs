use winit::event_loop::EventLoop;

use wasm_bindgen::prelude::*;
use wasm_bindgen::{FUNCTION_REGISTRY, FunctionRegistry};

use crate::{bindings::WINDOW, webview::State};

pub mod bindings;
mod home;
mod webview;

// Re-export bindings for convenience
pub use bindings::{Element, alert, console_log};

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
    let registry = &*FUNCTION_REGISTRY;

    println!("=== Generated JS Script ===\n{}\n=== End Script ===", registry.script());

    std::thread::spawn(app);
    let mut state = State::new(registry);
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}

fn app() {
    batch(|| {
        // Get document body using the lazily-initialized WINDOW static
        let document = WINDOW.with(|window| window.document());
        let body = document.body();

        // Create a container div
        let container = document.create_element("div".to_string());
        container.set_attribute("id".to_string(), "heap-demo".to_string());
        container.set_attribute("style".to_string(),
        "margin: 20px; padding: 15px; border: 2px solid #4CAF50; border-radius: 8px; background: #f9f9f9;".to_string());

        // Create a heading
        let heading = document.create_element("h2".to_string());
        heading.set_text_content("JSHeap Demo".to_string());
        heading.set_attribute(
            "style".to_string(),
            "color: #333; margin-top: 0;".to_string(),
        );
        container.append_child(heading);

        // Create a counter display
        let counter_display = document.create_element("p".to_string());
        counter_display.set_attribute("id".to_string(), "heap-counter".to_string());
        counter_display.set_attribute(
            "style".to_string(),
            "font-size: 24px; font-weight: bold; color: #2196F3;".to_string(),
        );
        counter_display.set_text_content("Counter: 0".to_string());
        container.append_child(counter_display.clone());

        // Create a button
        let button = document.create_element("button".to_string());
        button.set_text_content("Click me (heap-managed)".to_string());
        button.set_attribute("id".to_string(), "heap-button".to_string());
        button.set_attribute("style".to_string(),
        "padding: 10px 20px; font-size: 16px; cursor: pointer; background: #4CAF50; color: white; border: none; border-radius: 4px;".to_string());
        container.append_child(button);
        // Append container to body
        body.append_child(container);

        let counter_ref = counter_display.clone();
        // Demo 4: Event handling with heap refs
        let mut count = 0;
        body.add_event_listener(
            "click".to_string(),
            Box::new(move || {
                count += 1;

                // Update the counter display using the heap ref
                let start = std::time::Instant::now();
                counter_ref.set_text_content(format!("Counter: {}", count));
                let duration = start.elapsed();
                println!(
                    "Updated counter display in {:?} microseconds",
                    duration.as_micros()
                );
            }),
        );
    });

    // Keep running to handle events
    wait_for_js_event::<()>();
}
