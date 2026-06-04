use crate::ast::ImportStatic;
use proc_macro2::{Ident, TokenStream};
use quote::quote_spanned;

use super::common::generate_wry_call_js_function;
use super::js::namespace_prefix;

pub(super) fn generate_static(
    st: &ImportStatic,
    krate: &TokenStream,
    module: Option<&Ident>,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &st.vis;
    let rust_name = &st.rust_name;
    let ty = &st.ty;
    let span = rust_name.span();

    // Generate JavaScript code to access the static
    let js_code = generate_static_js_code(st, prefix);

    assert!(st.thread_local_v2);
    let js_call = generate_wry_call_js_function(
        krate,
        module,
        &js_code,
        quote_spanned! {span=> fn() -> #ty },
        quote_spanned! {span=> () },
        span,
    );

    // Generate a lazily-initialized thread-local static.
    // Type information is now passed through the generated JS-function call.
    Ok(quote_spanned! {span=>
        #vis static #rust_name: #krate::JsThreadLocal<#ty> = {
            // This can't be named __init for compat with older rustc versions
            // https://github.com/rust-lang/rust/issues/147006
            fn __init_wbg() -> #ty {
                #js_call
            }
            #krate::JsThreadLocal::new(__init_wbg)
        };
    })
}

/// Generate JavaScript code to access a static value
fn generate_static_js_code(st: &ImportStatic, prefix: &str) -> String {
    let js_name = &st.js_name;

    // Build the prefix with namespace if present
    let full_prefix = namespace_prefix(prefix, st.js_namespace.as_deref());

    format!("() => {full_prefix}{js_name}")
}
