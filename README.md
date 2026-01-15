# Wasm bindgen wry

An (largely) api compatible version of [wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) that uses [wry](https://github.com/tauri-apps/wry) to run the generated javascript code while your code runs natively.

## Why?

Wasm-bindgen is a fundamental tool for interacting with javascript and the dom, but it only works if you are compiling to wasm. If you want access to native apis like threads, file system, or networking you have to either go through an ipc boundary or give up on using wasm-bindgen. This library lets you use wasm bindgen from native code which lets you both:
- Use wasm-bindgen compatible libraries like [web-sys](https://crates.io/crates/web-sys), [js-sys](https://crates.io/crates/js-sys), and [gloo](https://crates.io/crates/gloo)!
- Use native apis like threads, file system, and networking!

## Usage
Add this to your Cargo.toml:

```toml
[dependencies]
wry-launch = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry" }

[patch.crates-io]
wasm-bindgen = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
wasm-bindgen-futures = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
js-sys = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
web-sys = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
wry-bindgen = { git = "https://github.com/DioxusLabs/wasm-bindgen-wry", tag = "v0.2.106" }
```

Then in your main.rs:

```rust
fn main() {
    // Start the wry app with a wasm bindgen context
    wry_launch::run(|| async {
        // Your code here
        std::future::pending::<()>().await;
    })
}
```

## Demos

The paint example from web-sys running unmodified from a native thread:

https://github.com/user-attachments/assets/a34c15f2-ff03-4a85-b447-f1aa0c6b924c

A modified version of dioxus web (that doesn't use the sledgehammer optimizations) running on a native thread:

https://github.com/user-attachments/assets/0d56b30b-9791-44cb-9487-e406bb891ef4

Yew's todoMVC example running unmodified from a native thread:

https://github.com/user-attachments/assets/d13183a4-4d62-44ae-854e-11830126ca15

Leafelet.js bindings:

https://github.com/user-attachments/assets/a7f8e816-c8d5-486d-9746-875e324224b7

Tiptap bindings:

https://github.com/user-attachments/assets/4c6ef57d-5f89-4a3a-a8f3-1559ef415162
