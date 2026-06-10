# `wasm-bindgen-futures-x`

Target-switching shim for `wasm-bindgen-futures`.

- On `wasm32`, this crate re-exports upstream `wasm-bindgen-futures`.
- On native targets, this crate re-exports `wasm-bindgen-futures-wry`.

The Rust crate name remains `wasm_bindgen_futures` so it can stand in for `wasm-bindgen-futures` in existing code.
