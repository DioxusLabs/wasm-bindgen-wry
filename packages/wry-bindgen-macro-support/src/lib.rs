//! wry-bindgen-macro-support - Implementation of the wasm_bindgen attribute macro
//!
//! This crate contains Wry code generation for the `#[wasm_bindgen]`
//! attribute macro. Parsing and macro API compatibility come from upstream
//! `wasm-bindgen-macro-support`.

mod codegen;

use proc_macro2::TokenStream;
use quote::quote;
use wasm_bindgen_macro_support::Diagnostic;

/// Expand the wasm_bindgen attribute macro.
///
/// This is the main entry point called by the proc-macro crate.
pub fn expand(attr: TokenStream, input: TokenStream) -> Result<TokenStream, Diagnostic> {
    let program = wasm_bindgen_macro_support::parse_with_tokens(attr, input)?;
    Ok(codegen::generate(&program)?)
}

/// One `kind = "literal"` argument to `link_to!` (e.g. `module = "./worker.js"`).
struct LinkToArg {
    kind: syn::Ident,
    value: syn::LitStr,
}

impl syn::parse::Parse for LinkToArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let kind: syn::Ident = input.parse()?;
        let _: syn::Token![=] = input.parse()?;
        let value: syn::LitStr = input.parse()?;
        Ok(Self { kind, value })
    }
}

/// Expand a `link_to!(kind = "literal")` invocation into a runtime call that
/// registers the referenced JS with the wry runtime and returns the URL the
/// WebView serves it from.
///
/// - `module`: `include_str!` the source-relative file and register its content.
/// - `inline_js`: register the literal `&'static str` directly.
/// - `raw_module`: resolve the opaque specifier verbatim.
///
/// The generated calls route through `::wasm_bindgen::__rt`, mirroring how
/// `#[wasm_bindgen]`-generated module specs reference the runtime.
pub fn expand_link_to(input: TokenStream) -> Result<TokenStream, Diagnostic> {
    let LinkToArg { kind, value } = syn::parse2(input).map_err(Diagnostic::from)?;
    let expanded = match kind.to_string().as_str() {
        // `include_str!` resolves relative to the call site's source file, so the
        // included content matches what `register_linked_module` hashes.
        "module" => quote! {
            { ::wasm_bindgen::__rt::register_linked_module(include_str!(#value)) }
        },
        "inline_js" => quote! {
            { ::wasm_bindgen::__rt::register_linked_module(#value) }
        },
        "raw_module" => quote! {
            { ::wasm_bindgen::__rt::link_to_raw_specifier(#value) }
        },
        other => {
            return Err(syn::Error::new(
                kind.span(),
                format!(
                    "unsupported `link_to!` argument `{other}`; expected `module`, `raw_module`, or `inline_js`"
                ),
            )
            .into());
        }
    };
    Ok(expanded)
}

/// Expand an internal wasm-bindgen class marker method.
pub fn expand_class_marker(
    attr: TokenStream,
    input: TokenStream,
) -> Result<TokenStream, Diagnostic> {
    let program = wasm_bindgen_macro_support::parse_class_marker_with_tokens(attr, input)?;
    Ok(codegen::generate(&program)?)
}
