//! wry-bindgen-macro-support - Implementation of the wasm_bindgen attribute macro
//!
//! This crate contains the parsing, AST, and code generation logic for the
//! `#[wasm_bindgen]` attribute macro that targets Wry's WebView.

mod ast;
mod codegen;
mod parser;

use proc_macro2::TokenStream;

pub use ast::*;
pub use parser::BindgenAttrs;

/// Expand the wasm_bindgen attribute macro.
///
/// This is the main entry point called by the proc-macro crate.
pub fn expand(attr: TokenStream, input: TokenStream) -> Result<TokenStream, syn::Error> {
    // Parse the input item
    let item: syn::Item = syn::parse2(input)?;

    // Parse the attribute arguments
    let attrs = parser::parse_attrs(attr)?;

    // Convert to our AST and generate code
    let mut program = ast::Program {
        attrs,
        ..Default::default()
    };
    ast::parse_item(&mut program, item)?;

    // Generate the output tokens
    codegen::generate(&program)
}
