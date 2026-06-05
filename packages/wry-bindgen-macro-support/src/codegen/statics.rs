use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote_spanned};
use wasm_bindgen_macro_support::ast::{ImportStatic, ImportString, ThreadLocal};

use super::common::{generate_js_reexport_spec, generate_wry_call_js_function, namespace_tokens};
use super::js::namespace_prefix;

pub(super) fn generate_static(
    st: &ImportStatic,
    js_namespace: Option<&[String]>,
    reexport: Option<&Option<String>>,
    krate: &TokenStream,
    module: Option<&Ident>,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &st.vis;
    let rust_name = &st.rust_name;
    let ty = &st.ty;
    let span = rust_name.span();

    // Generate JavaScript code to access the static
    let js_code = generate_static_js_code(st, js_namespace, prefix);

    let js_call = generate_wry_call_js_function(
        krate,
        module,
        &js_code,
        quote_spanned! {span=> fn() -> #ty },
        quote_spanned! {span=> () },
        span,
    );
    let reexport_tokens = if let Some(reexport) = reexport {
        let name = reexport.clone().unwrap_or_else(|| st.js_name.clone());
        let js_code = generate_static_js_expr(st, js_namespace, prefix);
        generate_js_reexport_spec(
            "__STATIC_REEXPORT_SPEC",
            quote_spanned! {span=> #name },
            namespace_tokens(None, span),
            module,
            &js_code,
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    // A plain `static` (no thread-local attribute) and the deprecated `thread_local`
    // both lower to `JsStatic`, which derefs to the value so it reads like the JS
    // global. `thread_local_v2` lowers to the `.with()`-accessed `JsThreadLocal`.
    if matches!(st.thread_local, Some(ThreadLocal::V1) | None) {
        let thread_local_ident = format_ident!("__WRY_BINDGEN_STATIC_{}", rust_name);
        return Ok(quote_spanned! {span=>
            #krate::__rt::std::thread_local! {
                static #thread_local_ident: #ty = #js_call;
            }

            #[allow(deprecated)]
            #vis static #rust_name: #krate::JsStatic<#ty> = #krate::JsStatic {
                __inner: &#thread_local_ident,
            };
            #reexport_tokens
        });
    }

    // Generate a lazily-initialized thread-local static (thread_local_v2).
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
        #reexport_tokens
    })
}

pub(super) fn generate_static_string(
    st: &ImportString,
    reexport: Option<&Option<String>>,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let vis = &st.vis;
    let rust_name = &st.rust_name;
    let ty = &st.ty;
    let value = &st.string;
    let span = rust_name.span();

    let js_call = generate_wry_call_js_function(
        krate,
        None,
        &format!("() => {value:?}"),
        quote_spanned! {span=> fn() -> #ty },
        quote_spanned! {span=> () },
        span,
    );
    let reexport_tokens = if let Some(reexport) = reexport {
        let name = reexport.clone().unwrap_or_else(|| rust_name.to_string());
        let js_code = format!("{value:?}");
        generate_js_reexport_spec(
            "__STATIC_STRING_REEXPORT_SPEC",
            quote_spanned! {span=> #name },
            namespace_tokens(None, span),
            None,
            &js_code,
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    if matches!(st.thread_local, ThreadLocal::V1) {
        let thread_local_ident = format_ident!("__WRY_BINDGEN_STATIC_STRING_{}", rust_name);
        return Ok(quote_spanned! {span=>
            #krate::__rt::std::thread_local! {
                static #thread_local_ident: #ty = #js_call;
            }

            #[allow(deprecated)]
            #vis static #rust_name: #krate::JsStatic<#ty> = #krate::JsStatic {
                __inner: &#thread_local_ident,
            };
            #reexport_tokens
        });
    }

    if !matches!(st.thread_local, ThreadLocal::V2) {
        return Err(syn::Error::new(
            span,
            "static strings require `#[wasm_bindgen(thread_local_v2)]`",
        ));
    }

    Ok(quote_spanned! {span=>
        #vis static #rust_name: #krate::JsThreadLocal<#ty> = {
            fn __init_wbg() -> #ty {
                #js_call
            }
            #krate::JsThreadLocal::new(__init_wbg)
        };
        #reexport_tokens
    })
}

/// Generate JavaScript code to access a static value
fn generate_static_js_code(
    st: &ImportStatic,
    js_namespace: Option<&[String]>,
    prefix: &str,
) -> String {
    format!(
        "() => {}",
        generate_static_js_expr(st, js_namespace, prefix)
    )
}

fn generate_static_js_expr(
    st: &ImportStatic,
    js_namespace: Option<&[String]>,
    prefix: &str,
) -> String {
    let js_name = &st.js_name;

    // Build the prefix with namespace if present
    let full_prefix = namespace_prefix(prefix, js_namespace);

    format!("{full_prefix}{js_name}")
}
