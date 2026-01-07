#!/bin/bash

# Generate public API for wasm-bindgen (the submodule)
echo "Generating wasm-bindgen public API..."
(cd wasm-bindgen && cargo public-api --omit blanket-impls,auto-trait-impls,auto-derived-impls > ../wasm-bindgen.api.txt 2>/dev/null)

# Generate public API for wry-bindgen (the replacement crate)
echo "Generating wry-bindgen public API..."
(cd wry-bindgen && cargo public-api --omit blanket-impls,auto-trait-impls,auto-derived-impls > ../wry-bindgen.api.txt 2>/dev/null)

# Filter out cosmetic differences and unstable APIs
filter_cosmetic() {
    grep -v "^pub fn " |            # Method signatures from trait impls (cosmetic formatting differences)
    grep -v "^pub type.*::Output = <&'static wasm_bindgen::JsValue" |          # Type aliases formatted differently
    grep -v "^pub type.*Prim\d" |          # Types from unstable convert traits
    grep -v "convert::" |           # Unstable convert module (low priority)
    grep -v "WasmRet" |             # Unstable convert types
    grep -v "WasmSlice" |           # Unstable convert types
    grep -v "WasmAbi" |             # Unstable convert traits
    grep -v "WasmPrimitive" |       # Unstable convert traits
    grep -v "WasmClosure" |         # Unstable closure traits
    grep -v "IntoWasmClosure" |     # Unstable closure traits
    grep -v "into_abi" |            # Unstable ABI methods
    grep -v "from_abi" |            # Unstable ABI methods
    grep -v "::Abi" |               # ABI type aliases
    grep -v "::Anchor" |            # Anchor type aliases
    grep -v "UnwrapThrowExt" |      # WASM-specific panic trait
    grep -v "JsStatic" |            # Deprecated
    grep -v "JsThreadLocal" |       # Thread local wrapper
    grep -v "impl core::ops.*for &wasm_bindgen::JsValue$" |  # impl Op for &JsValue (cosmetic: we use impl Op<&JsValue> for &JsValue)
    grep -v "impl<'a> core::cmp::PartialEq<&'a" |  # Explicit lifetime PartialEq (cosmetic: we use non-lifetime version)
    grep -v "?core::marker::Sized" |  # Unsized bounds (cosmetic difference)
    grep -v "^pub struct wasm_bindgen::JsError$" |  # JsError struct (we have #[repr(transparent)] prefix)
    grep -v "^pub struct wasm_bindgen::prelude::JsError$"  # JsError in prelude (same as above)
}

# Find APIs in wasm-bindgen but not in wry-bindgen
echo ""
echo "APIs in wasm-bindgen but NOT in wry-bindgen:"
echo "============================================="
comm -23 <(sort wasm-bindgen.api.txt | filter_cosmetic) <(sort wry-bindgen.api.txt | filter_cosmetic)

rm wasm-bindgen.api.txt wry-bindgen.api.txt