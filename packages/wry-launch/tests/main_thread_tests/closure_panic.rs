use wasm_bindgen::{Closure, wasm_bindgen};

/// A panic inside a Rust closure invoked from JS is caught at the callback
/// boundary and surfaces to JS as a thrown error carrying the panic message,
/// matching wasm-bindgen — rather than unwinding across the FFI boundary and
/// aborting the process.
pub(crate) fn test_closure_panic_surfaces_as_js_error() {
    #[wasm_bindgen(inline_js = r#"
        export function call_catching(cb) {
            try { cb(); return null; }
            catch (e) { return e.message; }
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = call_catching)]
        fn call_catching(cb: Closure<dyn FnMut()>) -> Option<String>;
    }

    let cb = Closure::new(Box::new(|| -> () {
        panic!("closure went boom");
    }) as Box<dyn FnMut()>);
    let message = call_catching(cb);
    assert_eq!(message.as_deref(), Some("closure went boom"));
}
