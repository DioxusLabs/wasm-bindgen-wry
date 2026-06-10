# `js-sys-x`

Target-switching shim for `js-sys`.

- On `wasm32`, this crate re-exports upstream `js-sys`.
- On native targets, this crate re-exports `js-sys-wry`.

The Rust crate name remains `js_sys` so it can stand in for `js-sys` in existing code.
