#!/usr/bin/env bash

set -euo pipefail

cargo_cmd=(cargo)
if [ -n "${CARGO_TOOLCHAIN:-}" ]; then
  cargo_cmd=(cargo "+${CARGO_TOOLCHAIN}")
fi

"${cargo_cmd[@]}" fmt \
  -p wry-launch \
  -p wry-bindgen-core \
  -p wry-bindgen \
  -p wry-bindgen-runtime \
  -p wry-bindgen-macro \
  -p wry-bindgen-macro-support \
  -p wasm-bindgen \
  -p wasm-bindgen-macro \
  -p wasm-bindgen-test \
  -p wasm-bindgen-test-macro \
  -p wasm-bindgen-test-crate-a \
  -p wasm-bindgen-test-crate-b \
  -p gloo \
  -p dioxus-web \
  -p yew \
  -p leptos-todomvc \
  -p piet \
  -p tiptap-example \
  -p openstreetmap \
  "$@"
