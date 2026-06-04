<div align="center">

  <h1><code>web-sys-x</code></h1>

  <p>
    <strong>Web APIs for the wasm-bindgen-x desktop fork. Drive the DOM from a native thread.</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/web-sys-x"><img src="https://img.shields.io/crates/v/web-sys-x.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/web-sys-x"><img src="https://img.shields.io/crates/d/web-sys-x.svg?style=flat-square" alt="Download" /></a>
    <a href="https://docs.rs/web-sys-x"><img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square" alt="docs.rs docs" /></a>
  </p>

  <h3>
    <a href="https://github.com/DioxusLabs/wasm-bindgen-wry"> Repo </a>
    <span> | </span>
    <a href="https://wasm-bindgen.github.io/wasm-bindgen/web-sys/index.html"> web-sys guide </a>
    <span> | </span>
    <a href="https://github.com/DioxusLabs/wasm-bindgen-wry/tree/main/examples"> Examples </a>
    <span> | </span>
    <a href="https://wasm-bindgen.github.io/wasm-bindgen/api/web_sys/"> API Docs </a>
  </h3>

</div>
<br>

Raw bindings to Web APIs for the [`wasm-bindgen-x`](https://crates.io/crates/wasm-bindgen-x) desktop fork. Drop this in via `[patch.crates-io]` and the same `web-sys` types your wasm crate uses light up against a native [`wry`](https://github.com/tauri-apps/wry) webview.

```rust
use core::f64;
use wasm_bindgen::prelude::*;

fn main() -> wry::Result<()> {
    wry_launch::run(|| async {
        draw();
        std::future::pending().await
    })
}

fn draw() {
    let document = web_sys::window().unwrap().document().unwrap();
    document
        .body()
        .unwrap()
        .set_inner_html(r#"<canvas id="c" height="150" width="150"></canvas>"#);

    let canvas: web_sys::HtmlCanvasElement =
        document.get_element_by_id("c").unwrap().dyn_into().unwrap();
    let ctx: web_sys::CanvasRenderingContext2d =
        canvas.get_context("2d").unwrap().unwrap().dyn_into().unwrap();

    ctx.begin_path();
    ctx.arc(75.0, 75.0, 50.0, 0.0, f64::consts::PI * 2.0).unwrap();
    ctx.stroke();
}
```

## ⭐️ Why?

Wasm-bindgen is the standard way to talk to JavaScript and the DOM, but it only works when compiling to wasm. If you want native APIs like threads, the filesystem, or networking you have to either cross an IPC boundary or give up `wasm-bindgen` entirely. This crate gives you both:

- Use wasm-bindgen-compatible libraries like [web-sys-x](https://crates.io/crates/web-sys-x), [js-sys-x](https://crates.io/crates/js-sys-x), and [gloo](https://crates.io/crates/gloo) from native code.
- Use native APIs — threads, the file system, networking — at the same time.
- Zero changes to existing `web-sys` code: same types, same methods, same feature flags.

## Get started

```toml
[dependencies]
wry-launch = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry" }
web-sys = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", features = ["Window", "Document", "Element", "HtmlCanvasElement", "CanvasRenderingContext2d"] }

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
<summary>Upstream <code>web-sys</code> documentation</summary>

# `web-sys`

Raw bindings to Web APIs for projects using `wasm-bindgen`.

- [The `web-sys` section of the `wasm-bindgen`
  guide](https://wasm-bindgen.github.io/wasm-bindgen/web-sys/index.html)
- [API Documentation](https://wasm-bindgen.github.io/wasm-bindgen/api/web_sys/)

## Crate features

This crate by default contains very little when compiled as almost all of its
exposed APIs are gated by Cargo features. The exhaustive list of features can be
found in `crates/web-sys/Cargo.toml`, but the rule of thumb for `web-sys` is
that each type has its own cargo feature (named after the type). Using an API
requires enabling the features for all types used in the API, and APIs should
mention in the documentation what features they require.

## How to add an interface

If you don't see a particular web API in `web-sys`, here is how to add it.

1. Copy the WebIDL specification of the API and place it in a new file in the
   `webidls/unstable` folder. You can often find the IDL by going to the MDN
   docs page for the API, scrolling to the bottom, clicking the
   "Specifications" link, and scrolling to the bottom of the specification
   page. For example, the bottom of the [MDN
   docs](https://developer.mozilla.org/en-US/docs/Web/API/MediaSession) on the
   MediaSession API takes you to the
   [spec](https://w3c.github.io/mediasession/#the-mediasession-interface). The
   [very bottom](https://w3c.github.io/mediasession/#idl-index) of _that_ page
   is the IDL.
2. Annotate the functions that can throw with `[Throws]`
3. `cd crates/web-sys`
4. Run `cargo run --release --package wasm-bindgen-webidl -- webidls src/features ./Cargo.toml`
5. Run `git add .` to add all the generated files into git.
6. Add an entry in CHANGELOG.md like the following

   ```md
   ...

   ## Unreleased

   ### Added

   ...

   * Added <your addition>
     [#1234](https://github.com/wasm-bindgen/wasm-bindgen/pull/1234) # <- link to your PR
   ```

</details>
