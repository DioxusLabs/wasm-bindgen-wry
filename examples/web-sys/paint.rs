//! Modified from https://github.com/wasm-bindgen/wasm-bindgen/tree/main/examples/paint to work with wry-bindgen

use core::f64;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wry_testing::run;

fn main() -> wry::Result<()> {
    run(|| async {
        app();
        std::future::pending().await
    })
}

fn app() {
    let window = web_sys::window().expect("should have a window in this context");
    let document = window.document().expect("window should have a document");

    // Below are some more advanced usages of the `Closure` type for closures
    // that need to live beyond our function call.

    // Add a clock with #current-time that updates every second.
    let body = document.body().expect("document should have a body");
    body.set_inner_html(r#"<canvas id="canvas" height="150" width="150"></canvas>"#);

    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .unwrap();
    document.body().unwrap().append_child(&canvas).unwrap();
    canvas.set_width(640);
    canvas.set_height(480);
    canvas.style().set_property("border", "solid").unwrap();
    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    let context = Rc::new(context);
    let pressed = Rc::new(Cell::new(false));
    {
        let context = context.clone();
        let pressed = pressed.clone();
        let closure = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MouseEvent| {
            context.begin_path();
            context.move_to(event.offset_x() as f64, event.offset_y() as f64);
            pressed.set(true);
        });
        canvas
            .add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }
    {
        let context = context.clone();
        let pressed = pressed.clone();
        let closure = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MouseEvent| {
            if pressed.get() {
                context.line_to(event.offset_x() as f64, event.offset_y() as f64);
                context.stroke();
                context.begin_path();
                context.move_to(event.offset_x() as f64, event.offset_y() as f64);
            }
        });
        canvas
            .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }
    {
        let closure = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MouseEvent| {
            pressed.set(false);
            context.line_to(event.offset_x() as f64, event.offset_y() as f64);
            context.stroke();
        });
        canvas
            .add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }
}
