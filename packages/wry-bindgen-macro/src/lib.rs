//! wry-bindgen-macro - Proc-macro for wasm_bindgen-style bindings
//!
//! This crate provides the `#[wasm_bindgen]` attribute macro that generates
//! code for Wry's WebView IPC protocol.

use proc_macro::TokenStream;
use quote::ToTokens;

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
        Err(err) => err.to_token_stream().into(),
    }
}

/// Internal class marker macro used by wasm-bindgen impl-method expansion.
#[proc_macro_attribute]
pub fn __wasm_bindgen_class_marker(attr: TokenStream, input: TokenStream) -> TokenStream {
    match wry_bindgen_macro_support::expand_class_marker(attr.into(), input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_token_stream().into(),
    }
}

/// Link to a JS file for use with workers/worklets.
///
/// Registers the referenced JS with the runtime and returns the URL the WebView
/// serves it from, so it can be handed to APIs like `Worker::new`.
///
/// # Example
///
/// ```ignore
/// use web_sys::Worker;
/// let worker = Worker::new(&wasm_bindgen::link_to!(module = "/src/worker.js"));
/// ```
#[proc_macro]
pub fn link_to(input: TokenStream) -> TokenStream {
    match wry_bindgen_macro_support::expand_link_to(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_token_stream().into(),
    }
}
