//! wry-bindgen-macro-support - Implementation of the wasm_bindgen attribute macro
//!
//! This crate contains Wry code generation for the `#[wasm_bindgen]`
//! attribute macro. Parsing and macro API compatibility come from upstream
//! `wasm-bindgen-macro-support`.

mod codegen;

use proc_macro2::TokenStream;
use wasm_bindgen_macro_support::Diagnostic;

/// Expand the wasm_bindgen attribute macro.
///
/// This is the main entry point called by the proc-macro crate.
pub fn expand(attr: TokenStream, input: TokenStream) -> Result<TokenStream, Diagnostic> {
    let program = wasm_bindgen_macro_support::parse_with_tokens(attr, input)?;
    Ok(codegen::generate(&program)?)
}

/// Expand an internal wasm-bindgen class marker method.
pub fn expand_class_marker(
    attr: TokenStream,
    input: TokenStream,
) -> Result<TokenStream, Diagnostic> {
    let program = wasm_bindgen_macro_support::parse_class_marker_with_tokens(attr, input)?;
    Ok(codegen::generate(&program)?)
}
