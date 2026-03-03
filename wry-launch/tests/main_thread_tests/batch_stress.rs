use wasm_bindgen::prelude::*;
use wasm_bindgen::wasm_bindgen;

/// Minimal reproduction of IPC buffer exhaustion from
/// https://github.com/DioxusLabs/wasm-bindgen-wry/issues/21
///
/// The original crash:
///   panicked at batch.rs:415: Failed to decode return value: U8BufferEmpty
///   data.is_empty()=true, fn_id=1297, is_batching=false, needs_flush=false
///
/// Root cause: When Closure callbacks are triggered by browser event dispatch
/// (setTimeout, scroll, resize, etc.), the timing between evaluate_script
/// and callback XHRs in the protocol handler causes response data to go
/// missing. Synchronous JS callbacks (for loops) do NOT trigger this.
pub(crate) async fn test_batch_stress_browser_event_callbacks() {
    use wasm_bindgen::Closure;

    #[wasm_bindgen(inline_js = r#"
        export function schedule_callbacks(cb, count) {
            for (let i = 0; i < count; i++) {
                setTimeout(() => cb(i), 0);
            }
        }
        export function wait_ms(ms) {
            return new Promise(resolve => setTimeout(resolve, ms));
        }
    "#)]
    extern "C" {
        fn schedule_callbacks(cb: &Closure<dyn FnMut(u32)>, count: u32);

        #[wasm_bindgen(catch)]
        async fn wait_ms(ms: u32) -> Result<JsValue, JsValue>;
    }

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let body = document.body().unwrap();

    let container = document.create_element("div").unwrap();
    body.append_child(&container).unwrap();

    let counter = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counter_clone = counter.clone();
    let document_clone = document.clone();
    let container_clone = container.clone();

    let callback = Closure::new(move |i: u32| {
        counter_clone.set(counter_clone.get() + 1);

        let item = document_clone.create_element("div").unwrap();
        item.set_attribute("class", "grid-item").unwrap();
        item.set_attribute(
            "style",
            &format!(
                "position:absolute;top:{}px;left:{}px;width:200px;height:280px",
                (i / 5) * 296,
                (i % 5) * 216,
            ),
        )
        .unwrap();

        let cover = document_clone.create_element("div").unwrap();
        cover
            .set_attribute("style", "width:200px;height:200px;background:#333")
            .unwrap();
        item.append_child(&cover).unwrap();

        let text = document_clone.create_element("div").unwrap();
        text.set_text_content(Some(&format!("Album {i}")));
        item.append_child(&text).unwrap();

        container_clone.append_child(&item).unwrap();
    });

    schedule_callbacks(&callback, 200);

    for _ in 0..20 {
        wait_ms(50).await.unwrap();
        let div = document.create_element("div").unwrap();
        div.set_text_content(Some("Rust-side element"));
        body.append_child(&div).unwrap();
        if counter.get() >= 200 {
            break;
        }
    }

    assert!(
        counter.get() >= 200,
        "Expected 200 callbacks, got {}",
        counter.get()
    );

    callback.forget();
}
