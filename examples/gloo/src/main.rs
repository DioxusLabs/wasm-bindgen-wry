use chrono::Timelike;
use futures_util::{future::ready, stream::StreamExt};
use gloo::timers::future::IntervalStream;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

pub fn main() {
    wry_testing::run(|| async {
        app();
        std::future::pending().await
    })
    .unwrap();
}

fn app() {
    console_error_panic_hook::set_once();

    let document = web_sys::window().unwrap_throw().document().unwrap_throw();
    let body = document.body().expect("document should have a body");
    body.set_inner_html(
        r###"
        <pre id="clock"></pre>
        <script src="pkg/clock.js"></script>
        <script>
         wasm_bindgen("pkg/clock_bg.wasm");
        </script>"###,
    );

    let el = document.get_element_by_id("clock").unwrap_throw();

    // render the date, then set it to re-render every second.
    render_date(&el);

    spawn_local(async move {
        IntervalStream::new(1_000)
            .for_each(|_| {
                render_date(&el);
                ready(())
            })
            .await;
    });
}

/// Render the date with the `:` flashing on and off every second into `el`.
fn render_date(el: &web_sys::Element) {
    // print the current date
    let date = chrono::Local::now();

    let format_str = if date.second().is_multiple_of(2) {
        "%Y-%m-%d %H %M"
    } else {
        "%Y-%m-%d %H:%M"
    };

    let date_str = date.format(format_str).to_string();

    // Set the contents of `el` to our date string
    el.set_text_content(Some(&date_str));
}
