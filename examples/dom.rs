//! Example application using wry-testing library

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Document, HtmlElement};
use wry_testing::run;

fn main() -> wry::Result<()> {
    run(||async  {
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
    body.set_inner_html(
        "<div id='script'>
        <p>
            The current time is:
            <span id='current-time'>...</span>
        </p>

        <div id='green-square'>
            <span>Click me!</span>
        </div>
        <p>
            You've clicked the green square
            <span id='num-clicks'>0</span>
            times
        </p>
        </div>",
    );

    spawn_local(async move {
        println!("hello from async task!");
    });

    setup_clicker(&document);
}

// We also want to count the number of times that our green square has been
// clicked. Our callback will update the `#num-clicks` div.
//
// This is pretty similar above, but showing how closures can also implement
// `FnMut()`.
fn setup_clicker(document: &Document) {
    let num_clicks = document
        .get_element_by_id("num-clicks")
        .expect("should have #num-clicks on the page");
    let mut clicks = 0;
    let a = Closure::<dyn FnMut()>::new(move || {
        clicks += 1;
        println!("Green square clicked {} times", clicks);
        num_clicks.set_inner_html(&clicks.to_string());
    });
    document
        .get_element_by_id("green-square")
        .expect("should have #green-square on the page")
        .dyn_ref::<HtmlElement>()
        .expect("#green-square be an `HtmlElement`")
        .set_onclick(Some(a.as_ref().unchecked_ref()));

    // See comments in `setup_clock` above for why we use `a.forget()`.
    a.forget();
}
