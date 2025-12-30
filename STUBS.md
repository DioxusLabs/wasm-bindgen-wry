# Stub/TODO/Placeholder Implementations

## Critical (will panic at runtime)

- [x] `wbg_cast` wasm memory casting stub - `wry-bindgen/src/lib.rs:64` (implemented using IDENTITY_CAST_SPEC)
- [ ] Unhandled callback function IDs - `wry-bindgen/src/runtime.rs:200`

## From agents.md (not yet started)

- [ ] Support for Clamped type
- [ ] Casting and type checking

## Defensive guards (intentional unreachable)

- [x] Reserved heap ID release guard - `wry-bindgen/src/batch.rs:88`
- [x] BatchableResult placeholders - `wry-bindgen/src/encode.rs:490,521,580`
- [x] Infallible type implementations - `wry-bindgen/src/value.rs:528,532`

## Intentional empty implementations (no action needed)

- [x] CloneForEncode marker trait for primitives - `wry-bindgen/src/encode.rs:594-607`
- [x] Error trait for JsError - `wry-bindgen/src/lib.rs:284`
- [x] Error trait for DecodeError - `wry-bindgen/src/ipc.rs:86`
- [x] Eq marker for JsValue - `wry-bindgen/src/value.rs:204`
- [x] Unit type encode/decode - `wry-bindgen/src/encode.rs:102-120`
- [x] impl_fnmut_stub macro - `wry-bindgen/src/encode.rs:643-835`
- [x] impl_fnmut_ref_stub macro - `wry-bindgen/src/encode.rs:839-915`
