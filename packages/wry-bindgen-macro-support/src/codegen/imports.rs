use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote_spanned};
use syn::ext::IdentExt;
use wasm_bindgen_macro_support::ast::{ImportFunction, ImportFunctionKind, MethodKind};

use super::common::{
    clippy_allows, extract_result_ok_type, generate_js_reexport_spec,
    generate_wry_call_js_function, is_unit_type, namespace_tokens,
};
use super::erasure::{
    GeneratedArgs, GenericEraseContext, add_js_call_bounds, add_js_call_bounds_to_generics,
    collect_constraining_type_params, generate_args, import_function_is_instance_method,
    receiver_impl_type, split_method_generics,
};
use super::js::{generate_js_code, mark_async_promise_handled_js_code};

fn import_ret(func: &ImportFunction) -> Option<&syn::Type> {
    func.function.ret.as_ref().map(|ret| &ret.r#type)
}

fn unsafety_tokens(func: &ImportFunction, span: proc_macro2::Span) -> TokenStream {
    if func.function.r#unsafe {
        quote_spanned! {span=> unsafe }
    } else {
        TokenStream::new()
    }
}

fn rust_attrs_tokens(func: &ImportFunction, span: proc_macro2::Span) -> TokenStream {
    let rust_attrs = &func.function.rust_attrs;
    quote_spanned! {span=> #(#rust_attrs)* #[allow(non_snake_case)] }
}

fn generate_function_reexport(
    func: &ImportFunction,
    reexport: Option<&Option<String>>,
    krate: &TokenStream,
    module: Option<&Ident>,
    js_code: &str,
) -> TokenStream {
    let Some(reexport) = reexport else {
        return TokenStream::new();
    };
    let span = func.rust_name.span();
    let name = reexport
        .clone()
        .unwrap_or_else(|| func.function.name.clone());
    generate_js_reexport_spec(
        "__FUNCTION_REEXPORT_SPEC",
        quote_spanned! {span=> #name },
        namespace_tokens(None, span),
        module,
        js_code,
        krate,
        span,
    )
}

pub(super) fn generate_function(
    func: &ImportFunction,
    js_namespace: Option<&[String]>,
    reexport: Option<&Option<String>>,
    type_names: &HashSet<String>,
    type_generics: &HashMap<String, syn::Generics>,
    vendor_prefixes: &HashMap<String, Vec<String>>,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
    module: Option<&Ident>,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &func.function.rust_vis;
    let rust_name = &func.rust_name;
    let unsafety = unsafety_tokens(func, rust_name.span());
    let span = rust_name.span();
    let call_generics = add_js_call_bounds(func, krate, true);
    let (fn_generics, _, fn_where_clause) = call_generics.split_for_impl();

    let args = generate_args(func, krate)?;
    let fn_params = &args.fn_params;
    let fn_types = &args.fn_type_list;
    let call_values = &args.call_value_list;

    let ret_type = match import_ret(func) {
        Some(ty) => quote_spanned! {span=> #ty },
        None => quote_spanned! {span=> () },
    };

    if func.function.r#async {
        let js_code_str = generate_js_code(func, js_namespace, vendor_prefixes, prefix, true);
        let js_code_str = mark_async_promise_handled_js_code(&js_code_str);
        let mut tokens = generate_async_function(
            func,
            type_generics,
            krate,
            js_sys,
            futures,
            module,
            &js_code_str,
            &args,
        )?;
        tokens.extend(generate_function_reexport(
            func,
            reexport,
            krate,
            module,
            &js_code_str,
        ));
        return Ok(tokens);
    }

    let js_code_str = generate_js_code(func, js_namespace, vendor_prefixes, prefix, false);
    let reexport_tokens = generate_function_reexport(func, reexport, krate, module, &js_code_str);

    let erase = GenericEraseContext::new(func);
    let call_ret_type = match import_ret(func) {
        Some(ty) if erase.type_uses_erased_params(ty) => {
            let concrete_ty = erase.concrete_type(ty, krate);
            quote_spanned! {span=> #concrete_ty }
        }
        Some(_) => ret_type.clone(),
        None => quote_spanned! {span=> () },
    };

    let func_body = if import_ret(func).is_some_and(|ty| erase.type_uses_erased_params(ty)) {
        let js_call = generate_wry_call_js_function(
            krate,
            module,
            &js_code_str,
            quote_spanned! {span=> fn(#(#fn_types),*) -> #call_ret_type },
            quote_spanned! {span=> (#(#call_values),*) },
            span,
        );
        quote_spanned! {span=>
            let __wry_ret = #js_call;
            unsafe {
                #krate::__rt::core::mem::transmute_copy(
                    &#krate::__rt::core::mem::ManuallyDrop::new(__wry_ret)
                )
            }
        }
    } else {
        let js_call = generate_wry_call_js_function(
            krate,
            module,
            &js_code_str,
            quote_spanned! {span=> fn(#(#fn_types),*) -> #call_ret_type },
            quote_spanned! {span=> (#(#call_values),*) },
            span,
        );
        quote_spanned! {span=> #js_call }
    };

    let rust_attrs = rust_attrs_tokens(func, span);
    let allows = clippy_allows();

    match &func.kind {
        ImportFunctionKind::Normal => {
            if let Some(ns) = js_namespace
                && ns.len() == 1
                && type_names.contains(&ns[0])
            {
                let (impl_type, impl_generics, mut method_generics) =
                    namespaced_class_impl_parts(func, &ns[0], type_generics);
                add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
                let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
                let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
                return Ok(quote_spanned! {span=>
                    impl #impl_generics #impl_type #impl_where_clause {
                        #allows
                        #rust_attrs
                        #vis #unsafety fn #rust_name #method_generics (#fn_params) -> #ret_type #method_where_clause {
                            #func_body
                        }
                    }
                    #reexport_tokens
                });
            }
            Ok(quote_spanned! {span=>
                #allows
                #rust_attrs
                #vis #unsafety fn #rust_name #fn_generics (#fn_params) -> #ret_type #fn_where_clause {
                    #func_body
                }
                #reexport_tokens
            })
        }
        ImportFunctionKind::Method { ty, kind, .. } if import_function_is_instance_method(func) => {
            let receiver_type = receiver_impl_type(ty)?;
            let (impl_generics, mut method_generics) = split_method_generics(&func.generics, ty);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            let method_args = if fn_params.is_empty() {
                quote_spanned! {span=> &self }
            } else {
                quote_spanned! {span=> &self, #fn_params }
            };

            let _ = kind;
            Ok(quote_spanned! {span=>
                impl #impl_generics #receiver_type #impl_where_clause {
                    #allows
                    #rust_attrs
                    #vis #unsafety fn #rust_name #method_generics (#method_args) -> #ret_type #method_where_clause {
                        #func_body
                    }
                }
                #reexport_tokens
            })
        }
        ImportFunctionKind::Method {
            ty,
            kind: MethodKind::Constructor,
            ..
        } => {
            let (impl_type, impl_generics, mut method_generics) =
                constructor_impl_parts(func, ty, type_generics);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #rust_attrs
                    #vis #unsafety fn #rust_name #method_generics (#fn_params) -> #ret_type #method_where_clause {
                        #func_body
                    }
                }
                #reexport_tokens
            })
        }
        ImportFunctionKind::Method { ty, .. } => {
            let (impl_type, impl_generics, mut method_generics) = class_impl_parts_for_ty(func, ty);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #rust_attrs
                    #vis #unsafety fn #rust_name #method_generics (#fn_params) -> #ret_type #method_where_clause {
                        #func_body
                    }
                }
                #reexport_tokens
            })
        }
    }
}

fn class_name_from_ty(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.unraw().to_string())
}

fn constructor_impl_parts(
    func: &ImportFunction,
    rust_ty: &syn::Type,
    type_generics: &HashMap<String, syn::Generics>,
) -> (syn::Type, syn::Generics, syn::Generics) {
    if let Some(class) = class_name_from_ty(rust_ty) {
        namespaced_class_impl_parts(func, &class, type_generics)
    } else {
        class_impl_parts_for_ty(func, rust_ty)
    }
}

fn class_impl_parts_for_ty(
    func: &ImportFunction,
    rust_ty: &syn::Type,
) -> (syn::Type, syn::Generics, syn::Generics) {
    let (impl_generics, method_generics) = split_method_generics(&func.generics, rust_ty);
    (rust_ty.clone(), impl_generics, method_generics)
}

fn namespaced_class_impl_parts(
    func: &ImportFunction,
    class: &str,
    type_generics: &HashMap<String, syn::Generics>,
) -> (syn::Type, syn::Generics, syn::Generics) {
    // Upstream hoists class generics for constructors/static functions whose
    // return type is the class itself, e.g. `fn new<T>() -> Promise<T>`.
    if type_generics
        .get(class)
        .is_some_and(|generics| !generics.params.is_empty())
        && let Some(class_type) = return_type_matching_class(func, class)
    {
        let (impl_generics, method_generics) = split_method_generics(&func.generics, &class_type);
        return (class_type, impl_generics, method_generics);
    }

    let class_ident = format_ident!("{}", class);
    (
        syn::parse_quote!(#class_ident),
        syn::Generics::default(),
        func.generics.clone(),
    )
}

fn return_type_matching_class(func: &ImportFunction, class: &str) -> Option<syn::Type> {
    let ret = import_ret(func)
        .and_then(|ret| extract_result_ok_type(ret).or_else(|| Some(ret.clone())))?;
    let syn::Type::Path(path) = &ret else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let segment = path.path.segments.last()?;
    if segment.ident != class {
        return None;
    }
    if !matches!(
        segment.arguments,
        syn::PathArguments::AngleBracketed(ref args) if !args.args.is_empty()
    ) {
        return None;
    }

    let known_type_params: HashSet<String> = func
        .generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect();
    let mut found = HashSet::new();
    if !collect_constraining_type_params(&ret, &known_type_params, &mut found) || found.is_empty() {
        return None;
    }

    Some(ret)
}

fn generate_async_function(
    func: &ImportFunction,
    type_generics: &HashMap<String, syn::Generics>,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
    module: Option<&Ident>,
    js_code_str: &str,
    args: &GeneratedArgs,
) -> syn::Result<TokenStream> {
    let vis = &func.function.rust_vis;
    let rust_name = &func.rust_name;
    let unsafety = unsafety_tokens(func, rust_name.span());
    let span = rust_name.span();
    let rust_attrs = &func.function.rust_attrs;
    let call_generics = add_js_call_bounds(func, krate, false);
    let (fn_generics, _, fn_where_clause) = call_generics.split_for_impl();

    let fn_params = &args.fn_params;
    let fn_types = &args.fn_type_list;
    let call_values = &args.call_value_list;
    let js_call = generate_wry_call_js_function(
        krate,
        module,
        js_code_str,
        quote_spanned! {span=> fn(#(#fn_types),*) -> #js_sys::Promise },
        quote_spanned! {span=> (#(#call_values),*) },
        span,
    );
    let async_body = quote_spanned! {span=>
        {
            let __wry_promise = #js_call;
            #futures::JsFuture::from(__wry_promise).await
        }
    };

    let (ret_clause, ret_handling) = match import_ret(func) {
        Some(ty) => {
            if let Some(ok_type) = extract_result_ok_type(ty) {
                if is_unit_type(&ok_type) {
                    (
                        quote_spanned! {span=> -> #ty },
                        quote_spanned! {span=> .map(|_| ()) },
                    )
                } else {
                    (
                        quote_spanned! {span=> -> #ty },
                        quote_spanned! {span=>
                            .map(|v| {
                                <#ok_type as #krate::convert::TryFromJsValue>::try_from_js_value(v)
                                    .expect("async function returned incompatible value")
                            })
                        },
                    )
                }
            } else {
                (
                    quote_spanned! {span=> -> #ty },
                    quote_spanned! {span=>
                        .map(|v| {
                            <#ty as #krate::convert::TryFromJsValue>::try_from_js_value(v)
                                .expect("async function returned incompatible value")
                        })
                        .expect("async function failed")
                    },
                )
            }
        }
        None => (
            quote_spanned! {span=> },
            quote_spanned! {span=> .expect("async function failed"); },
        ),
    };

    let allows = clippy_allows();

    match &func.kind {
        ImportFunctionKind::Normal => Ok(quote_spanned! {span=>
            #allows
            #(#rust_attrs)*
            #vis #unsafety async fn #rust_name #fn_generics (#fn_params) #ret_clause #fn_where_clause {
                #async_body #ret_handling
            }
        }),
        ImportFunctionKind::Method { ty, .. } if import_function_is_instance_method(func) => {
            let receiver_type = receiver_impl_type(ty)?;
            let (impl_generics, mut method_generics) = split_method_generics(&func.generics, ty);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, false);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            let method_args = if fn_params.is_empty() {
                quote_spanned! {span=> &self }
            } else {
                quote_spanned! {span=> &self, #fn_params }
            };

            Ok(quote_spanned! {span=>
                impl #impl_generics #receiver_type #impl_where_clause {
                    #allows
                    #(#rust_attrs)*
                    #vis #unsafety async fn #rust_name #method_generics (#method_args) #ret_clause #method_where_clause {
                        #async_body #ret_handling
                    }
                }
            })
        }
        ImportFunctionKind::Method {
            ty,
            kind: MethodKind::Constructor,
            ..
        } => {
            let (impl_type, impl_generics, mut method_generics) =
                constructor_impl_parts(func, ty, type_generics);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, false);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #(#rust_attrs)*
                    #vis #unsafety async fn #rust_name #method_generics (#fn_params) #ret_clause #method_where_clause {
                        #async_body #ret_handling
                    }
                }
            })
        }
        ImportFunctionKind::Method { ty, .. } => {
            let (impl_type, impl_generics, mut method_generics) = class_impl_parts_for_ty(func, ty);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, false);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #(#rust_attrs)*
                    #vis #unsafety async fn #rust_name #method_generics (#fn_params) #ret_clause #method_where_clause {
                        #async_body #ret_handling
                    }
                }
            })
        }
    }
}
