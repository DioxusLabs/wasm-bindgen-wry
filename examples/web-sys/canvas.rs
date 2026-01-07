//! Modified from https://github.com/wasm-bindgen/wasm-bindgen/tree/main/examples/canvas to work with wry-bindgen

use core::f64;

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
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| ())
        .unwrap();

    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();

    context.begin_path();

    // Draw the outer circle.
    context
        .arc(75.0, 75.0, 50.0, 0.0, f64::consts::PI * 2.0)
        .unwrap();

    // Draw the mouth.
    context.move_to(110.0, 75.0);
    context.arc(75.0, 75.0, 35.0, 0.0, f64::consts::PI).unwrap();

    // Draw the left eye.
    context.move_to(65.0, 65.0);
    context
        .arc(60.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
        .unwrap();

    // Draw the right eye.
    context.move_to(95.0, 65.0);
    context
        .arc(90.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
        .unwrap();

    context.stroke();
}
