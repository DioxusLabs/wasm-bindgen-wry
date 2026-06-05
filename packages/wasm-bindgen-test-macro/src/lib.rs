//! Desktop `#[wasm_bindgen_test]` attribute macro.
//!
//! Upstream wasm-bindgen-test emits a `#[cfg(target_family = "wasm")]` `__wbgt_` export
//! that calls into a JS `Context`; on a native target that expands to nothing usable.
//! This expansion instead registers the test with the wry-bindgen native harness through
//! `inventory`: the annotated function is emitted unchanged, and a companion
//! `RegisteredTest` is submitted so the `upstream_tests` harness can discover and run it.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Expr, ItemFn, Lit, Meta, ReturnType, parse_macro_input};

#[proc_macro_attribute]
pub fn wasm_bindgen_test(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut func = parse_macro_input!(input as ItemFn);

    // Pull the test-only attributes out so the emitted function compiles as a plain fn.
    let mut should_panic: Option<Option<String>> = None;
    let mut ignore = false;
    let mut kept_attrs: Vec<Attribute> = Vec::new();
    for attr in func.attrs.drain(..) {
        if attr.path().is_ident("should_panic") {
            should_panic = Some(parse_should_panic(&attr));
        } else if attr.path().is_ident("ignore") {
            ignore = true;
        } else {
            kept_attrs.push(attr);
        }
    }
    func.attrs = kept_attrs;

    let ident = func.sig.ident.clone();
    let is_async = func.sig.asyncness.is_some();
    let returns_result = matches!(func.sig.output, ReturnType::Type(..));

    let await_tok = if is_async { quote!(.await) } else { quote!() };
    // Invoke the original function, turning a `Result` return into a panic so the
    // registered thunk is always `-> ()`.
    let invoke = if returns_result {
        quote! {
            match #ident()#await_tok {
                ::core::result::Result::Ok(()) => {}
                ::core::result::Result::Err(__e) => {
                    ::core::panic!("test returned Err: {:?}", __e)
                }
            }
        }
    } else {
        quote! { #ident()#await_tok; }
    };

    let kind = if is_async {
        quote! {
            ::wasm_bindgen_test::__rt::TestKind::Async(
                || -> ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<Output = ()>>> {
                    ::std::boxed::Box::pin(async move { #invoke })
                }
            )
        }
    } else {
        quote! { ::wasm_bindgen_test::__rt::TestKind::Sync(|| { #invoke }) }
    };

    let should_panic_tok = match &should_panic {
        None => quote! { ::core::option::Option::None },
        Some(None) => quote! { ::core::option::Option::Some(::core::option::Option::None) },
        Some(Some(msg)) => {
            quote! { ::core::option::Option::Some(::core::option::Option::Some(#msg)) }
        }
    };

    quote! {
        #func

        ::wasm_bindgen_test::__rt::inventory::submit! {
            ::wasm_bindgen_test::__rt::RegisteredTest {
                module_path: ::core::module_path!(),
                name: ::core::stringify!(#ident),
                should_panic: #should_panic_tok,
                ignore: #ignore,
                kind: #kind,
            }
        }
    }
    .into()
}

/// Parse the expected substring from a `should_panic` attribute, if any.
///
/// `#[should_panic]` → `None`; `#[should_panic = "msg"]` and
/// `#[should_panic(expected = "msg")]` → `Some("msg")`.
fn parse_should_panic(attr: &Attribute) -> Option<String> {
    match &attr.meta {
        Meta::Path(_) => None,
        Meta::NameValue(nv) => lit_string(&nv.value),
        Meta::List(list) => {
            let mut found = None;
            let _ = list.parse_nested_meta(|meta| {
                if meta.path.is_ident("expected") {
                    let lit: syn::LitStr = meta.value()?.parse()?;
                    found = Some(lit.value());
                }
                Ok(())
            });
            found
        }
    }
}

fn lit_string(expr: &Expr) -> Option<String> {
    if let Expr::Lit(syn::ExprLit {
        lit: Lit::Str(s), ..
    }) = expr
    {
        Some(s.value())
    } else {
        None
    }
}
