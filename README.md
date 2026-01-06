# Wasm bindgen wry

An (largely) api compatible version of [wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) that uses [wry](https://github.com/tauri-apps/wry) to run the generated javascript code while your code runs natively.

## Why?

Wasm-bindgen is a fundamental tool for interacting with javascript and the dom, but it only works if you are compiling to wasm. If you want access to native apis like threads, file system, or networking you have to either go through an ipc boundary or give up on using wasm-bindgen. This library lets you use wasm bindgen from native code which lets you both:
- Use wasm-bindgen compatible libraries like [web-sys](https://crates.io/crates/web-sys), [js-sys](https://crates.io/crates/js-sys), and [gloo](https://crates.io/crates/gloo)!
- Use native apis like threads, file system, and networking!

## Demos

The paint example from web-sys running unmodified from a native thread:

https://github.com/user-attachments/assets/a34c15f2-ff03-4a85-b447-f1aa0c6b924c

Yew's todoMVC example running unmodified from a native thread:



A modified version of dioxus web (that doesn't use the sledgehammer optimizations) running on a native thread:

