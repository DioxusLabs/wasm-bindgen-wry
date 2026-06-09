# wry-bindgen-core

`wry-bindgen-core` is the minimal, stable API that [`wry-bindgen`](../wry-bindgen) builds on. It exposes typed handles for talking to JavaScript: functions you can call, Rust values you can store and borrow back, and closures you can hand to JavaScript.

```rust, no_run
# #[derive(Default)]
# struct Counter;
# impl Counter { fn tick(&mut self) {} }
use wry_bindgen_core::with_runtime;

// Store a native Rust value and get back an opaque handle...
let handle = with_runtime(|rt| rt.insert_object(Counter::default()));

// ...then borrow it back by handle later.
with_runtime(|rt| {
    let mut counter = rt.object_mut::<Counter>(handle).expect("counter handle");
    counter.tick();
});
```

## What it provides

- `JsFunction<F>` — a typed handle to a JavaScript function. `.call(args)` runs the round-trip and returns the typed result.
- `Runtime` and `with_runtime` — a typed object store: insert a Rust value for an `ObjectHandle`, borrow it back, or remove it.
- `LazyCell<T>` / `JsThreadLocal<T>` — typed runtime-local JavaScript values, such as the global `window`.
- `CallbackKey<F>` — registers a Rust closure with JavaScript, with Rust-owned, JS-owned, and JS-owned-once policies.
- `Clamped`, `BatchableResult`, and `RequireFlush` — markers used when encoding arguments and return values.

Most of these are driven by the code `wry-bindgen`'s macro generates; the object store reached through `with_runtime` is the part you tend to use directly.
