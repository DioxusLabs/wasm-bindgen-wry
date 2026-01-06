// Copyright 2019 the Piet Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Basic example of rendering in the browser modified to run with wry-bindgen

use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, window};

use piet::{RenderContext, samples};
use piet_web::WebRenderContext;

//TODO: figure out how to dynamically select the sample?
const SAMPLE_PICTURE_NO: usize = 11;

fn main() {
    wry_testing::run(|| async {
        window()
            .unwrap()
            .document()
            .unwrap()
            .body()
            .unwrap()
            .set_inner_html(r#"<canvas id="canvas"></canvas>"#);
        run();
        std::future::pending::<()>().await;
    })
    .unwrap();
}

pub fn run() {
    let window = window().unwrap();
    let canvas = window
        .document()
        .unwrap()
        .get_element_by_id("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();

    let sample = samples::get::<WebRenderContext>(SAMPLE_PICTURE_NO).unwrap();
    let dpr = window.device_pixel_ratio();
    canvas.set_width((canvas.offset_width() as f64 * dpr) as u32);
    canvas.set_height((canvas.offset_height() as f64 * dpr) as u32);
    let _ = context.scale(dpr, dpr);

    let mut piet_context = WebRenderContext::new(context, window);

    sample.draw(&mut piet_context).unwrap();
    piet_context.finish().unwrap();
}
