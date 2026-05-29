<div align="center">

  <h1><code>wasm-bindgen-futures-x</code></h1>

  <p>
    <strong>Bridge Rust <code>Future</code>s and the webview's JavaScript <code>Promise</code>s, while your Rust runs natively.</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/wasm-bindgen-futures-x"><img src="https://img.shields.io/crates/v/wasm-bindgen-futures-x.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/wasm-bindgen-futures-x"><img src="https://img.shields.io/crates/d/wasm-bindgen-futures-x.svg?style=flat-square" alt="Download" /></a>
    <a href="https://docs.rs/wasm-bindgen-futures-x"><img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square" alt="docs.rs docs" /></a>
  </p>

  <h3>
    <a href="https://github.com/DioxusLabs/wasm-bindgen-wry"> Repo </a>
    <span> | </span>
    <a href="https://wasm-bindgen.github.io/wasm-bindgen/"> wasm-bindgen guide </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/wasm-bindgen-wry/tree/main/examples"> Examples </a>
    <span> | </span>
    <a href="https://wasm-bindgen.github.io/wasm-bindgen/api/wasm_bindgen_futures/"> API Docs </a>
  </h3>

</div>
<br>

[`wasm-bindgen-futures`](https://crates.io/crates/wasm-bindgen-futures) for the [`wasm-bindgen-x`](https://crates.io/crates/wasm-bindgen-x) desktop fork. `.await` a JS `Promise` directly from your native main thread, with `spawn_local`, `JsFuture`, and `future_to_promise` all working the same as upstream.

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
    export async function add_async(a, b) {
        return new Promise(resolve => setTimeout(() => resolve(a + b), 100));
    }
")]
extern "C" {
    async fn add_async(a: u32, b: u32) -> JsValue;
}

fn main() -> wry::Result<()> {
    wry_launch::run(|| async {
        let result = add_async(2, 3).await;
        println!("got {} from JS", result.as_f64().unwrap());
    })
}
```

## ⭐️ Why?

Wasm-bindgen is the standard way to talk to JavaScript and the DOM, but it only works when compiling to wasm. If you want native APIs like threads, the filesystem, or networking you have to either cross an IPC boundary or give up `wasm-bindgen` entirely. This crate gives you both:

- Use wasm-bindgen-compatible libraries like [web-sys-x](https://crates.io/crates/web-sys-x), [js-sys-x](https://crates.io/crates/js-sys-x), and [gloo](https://crates.io/crates/gloo) from native code.
- Use native APIs — threads, the file system, networking — at the same time.
- `.await` a `Promise` directly; `spawn_local` and `JsFuture` work unchanged.

## Get started

```toml
[dependencies]
wry-launch = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry" }
wasm-bindgen-futures = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry" }

[patch.crates-io]
wasm-bindgen = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
wasm-bindgen-futures = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
js-sys = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
web-sys = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
wry-bindgen = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
```

## web-sys paint, running unmodified from a native thread

https://github.com/user-attachments/assets/a34c15f2-ff03-4a85-b447-f1aa0c6b924c

## Dioxus web on the desktop

A modified version of dioxus-web (without the sledgehammer optimizations) running on a native thread.

https://github.com/user-attachments/assets/0d56b30b-9791-44cb-9487-e406bb891ef4

## Yew's TodoMVC, unmodified

https://github.com/user-attachments/assets/d13183a4-4d62-44ae-854e-11830126ca15

## Leaflet.js bindings

https://github.com/user-attachments/assets/a7f8e816-c8d5-486d-9746-875e324224b7

## Tiptap bindings

https://github.com/user-attachments/assets/4c6ef57d-5f89-4a3a-a8f3-1559ef415162

## License

This project is licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

<details>
<summary>Upstream <code>wasm-bindgen-futures</code> documentation</summary>

# `wasm-bindgen-futures`

[API Documentation][docs]

This crate bridges the gap between Rust `Future`s and JavaScript `Promise`s.

As of this version the implementation lives in [`js-sys`] under the `futures`
feature. Depending on this crate automatically activates that feature, which
gives `js_sys::Promise` a first-class `IntoFuture` implementation — meaning
you can `.await` any `Promise` directly:

```rust
use js_sys::Promise;
use wasm_bindgen::prelude::*;

async fn example(promise: Promise) -> Result<JsValue, JsValue> {
    promise.await
}
```

All public items from the previous API are re-exported unchanged for backwards
compatibility:

- [`JsFuture`] — convert a `Promise` into a named `Future` type
- [`spawn_local`] — run a `Future<Output = ()>` on the JS microtask queue
- [`future_to_promise`] — convert a `Future` into a JS `Promise`
- [`future_to_promise_typed`] — typed variant of `future_to_promise`

Under the feature flag `futures-core-03-stream` there is support for
`AsyncIterator` to `Stream` conversion via `JsStream`.

See the [API documentation][docs] for more info.

[docs]: https://wasm-bindgen.github.io/wasm-bindgen/api/wasm_bindgen_futures/
[`js-sys`]: https://docs.rs/js-sys
[`JsFuture`]: https://docs.rs/js-sys/*/js_sys/futures/struct.JsFuture.html
[`spawn_local`]: https://docs.rs/js-sys/*/js_sys/futures/fn.spawn_local.html
[`future_to_promise`]: https://docs.rs/js-sys/*/js_sys/futures/fn.future_to_promise.html
[`future_to_promise_typed`]: https://docs.rs/js-sys/*/js_sys/futures/fn.future_to_promise_typed.html

</details>
