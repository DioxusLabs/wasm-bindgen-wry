# `web-sys-x`

Target-switching shim for `web-sys`.

- On `wasm32`, this crate re-exports upstream `web-sys`.
- On native targets, this crate re-exports `web-sys-wry`.

The Rust crate name remains `web_sys` so it can stand in for `web-sys` in existing code.
