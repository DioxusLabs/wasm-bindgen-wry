//! wry-bindgen-macro - Proc-macro for wasm_bindgen-style bindings
//!
//! This crate provides the `#[wasm_bindgen]` attribute macro that generates
//! code for Wry's WebView IPC protocol.

use proc_macro::TokenStream;

/// The main wasm_bindgen attribute macro.
///
/// This macro can be applied to `extern "C"` blocks to import JavaScript
/// functions and types, using the same syntax as the original wasm-bindgen.
///
/// # Example
///
/// ```ignore
/// use wry_bindgen::prelude::*;
///
/// #[wasm_bindgen]
/// extern "C" {
///     // Import a type
///     #[wasm_bindgen(extends = Node)]
///     pub type Element;
///
///     // Import a method
///     #[wasm_bindgen(method, js_name = getAttribute)]
///     pub fn get_attribute(this: &Element, name: &str) -> Option<String>;
///
///     // Import a getter
///     #[wasm_bindgen(method, getter)]
///     pub fn id(this: &Element) -> String;
///
///     // Import a constructor
///     #[wasm_bindgen(constructor)]
///     pub fn new() -> Element;
/// }
/// ```
#[proc_macro_attribute]
pub fn wasm_bindgen(attr: TokenStream, input: TokenStream) -> TokenStream {
    match wry_bindgen_macro_support::expand(attr.into(), input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Link to a JS file for use with workers/worklets.
///
/// This macro is only meaningful in WASM contexts. When running outside of WASM,
/// it will panic at runtime.
///
/// # Example
///
/// ```ignore
/// use web_sys::Worker;
/// let worker = Worker::new(&wasm_bindgen::link_to!(module = "/src/worker.js"));
/// ```
#[proc_macro]
pub fn link_to(_input: TokenStream) -> TokenStream {
    quote::quote! {
        panic!("link_to! cannot be used when running outside of wasm")
    }
    .into()
}
