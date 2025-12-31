use wry_testing::wait_for_js_result;
use leptos::prelude::*;

pub fn main() {
    wry_testing::run(|| {
        app();
        wait_for_js_result::<i32>();
    })
    .unwrap();
}

fn app() {
    leptos::mount::mount_to_body(|| view! { <p>"Hello, world!"</p> })
}