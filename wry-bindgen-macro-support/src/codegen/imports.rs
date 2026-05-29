use std::collections::{HashMap, HashSet};

use crate::ast::{ImportFunction, ImportFunctionKind};
use proc_macro2::TokenStream;
use quote::{format_ident, quote_spanned};

use super::common::{clippy_allows, extract_result_ok_type, is_unit_type};
use super::erasure::{
    GeneratedArgs, GenericEraseContext, add_js_call_bounds, add_js_call_bounds_to_generics,
    collect_constraining_type_params, generate_args, receiver_impl_type, split_method_generics,
};
use super::js::{async_promise_guard_js_code, generate_js_code};

pub(super) fn generate_function(
    func: &ImportFunction,
    type_names: &HashSet<String>,
    type_generics: &HashMap<String, syn::Generics>,
    vendor_prefixes: &std::collections::HashMap<String, Vec<String>>,
    krate: &TokenStream,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &func.vis;
    let rust_name = &func.rust_name;
    let span = rust_name.span();
    let call_generics = add_js_call_bounds(func, krate, true);
    let (fn_generics, _, fn_where_clause) = call_generics.split_for_impl();

    // Generate argument lists
    let args = generate_args(func, krate)?;
    let fn_params = &args.fn_params;
    let fn_types = &args.fn_type_list;
    let call_values = &args.call_value_list;

    // Generate return type
    let ret_type = match &func.ret {
        Some(ty) => quote_spanned! {span=> #ty },
        None => quote_spanned! {span=> () },
    };

    // Async imports return a JS Promise and are awaited through js-sys'
    // JsFuture implementation. The promise is wrapped so it owns a rejection
    // handler from creation, keeping an already-rejected promise observed
    // across the Rust-to-JS round-trip until JsFuture attaches its handlers.
    if func.is_async {
        let js_code_str = generate_js_code(func, vendor_prefixes, prefix, true);
        let js_code_str = async_promise_guard_js_code(&js_code_str);
        return generate_async_function(func, type_generics, krate, &js_code_str, &args);
    }

    // For non-async functions, generate a simple closure that returns a constant string
    let js_code_str = generate_js_code(func, vendor_prefixes, prefix, false);

    let erase = GenericEraseContext::new(func);
    let call_ret_type = match &func.ret {
        Some(ty) if erase.type_uses_erased_params(ty) => {
            let concrete_ty = erase.concrete_type(ty, krate);
            quote_spanned! {span=> #concrete_ty }
        }
        Some(_) => ret_type.clone(),
        None => quote_spanned! {span=> () },
    };

    // Generate the function body
    let func_body = if func
        .ret
        .as_ref()
        .is_some_and(|ty| erase.type_uses_erased_params(ty))
    {
        quote_spanned! {span=>
            let __wry_ret = #krate::__wry_call_js_function!(
                #js_code_str,
                fn(#(#fn_types),*) -> #call_ret_type,
                (#(#call_values),*)
            );
            unsafe {
                ::core::mem::transmute_copy(
                    &::core::mem::ManuallyDrop::new(__wry_ret)
                )
            }
        }
    } else {
        quote_spanned! {span=>
            #krate::__wry_call_js_function!(#js_code_str, fn(#(#fn_types),*) -> #call_ret_type, (#(#call_values),*))
        }
    };

    // Get the rust attributes to forward (like #[cfg(...)] and #[doc = "..."])
    let rust_attrs = func.fn_rust_attrs();
    let allows = clippy_allows();

    // Generate the full function based on kind
    match &func.kind {
        ImportFunctionKind::Normal => {
            // Check if this function has a single-element js_namespace that matches a type
            // defined in this extern block. If so, generate as a static method to avoid collisions.
            if let Some(ns) = &func.js_namespace
                && ns.len() == 1
                && type_names.contains(&ns[0])
            {
                let (impl_type, impl_generics, mut method_generics) =
                    class_impl_parts(func, &ns[0], type_generics);
                add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
                let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
                let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
                return Ok(quote_spanned! {span=>
                    impl #impl_generics #impl_type #impl_where_clause {
                        #allows
                        #rust_attrs
                        #vis fn #rust_name #method_generics (#fn_params) -> #ret_type #method_where_clause {
                            #func_body
                        }
                    }
                });
            }
            Ok(quote_spanned! {span=>
                #allows
                #rust_attrs
                #vis fn #rust_name #fn_generics (#fn_params) -> #ret_type #fn_where_clause {
                    #func_body
                }
            })
        }
        ImportFunctionKind::Method { receiver }
        | ImportFunctionKind::Getter { receiver, .. }
        | ImportFunctionKind::Setter { receiver, .. }
        | ImportFunctionKind::IndexingGetter { receiver }
        | ImportFunctionKind::IndexingSetter { receiver }
        | ImportFunctionKind::IndexingDeleter { receiver } => {
            // Extract the type name from the receiver
            let receiver_type = receiver_impl_type(receiver)?;
            let (impl_generics, mut method_generics) =
                split_method_generics(&func.generics, receiver);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();

            // Build method signature with optional additional args
            let method_args = if fn_params.is_empty() {
                quote_spanned! {span=> &self }
            } else {
                quote_spanned! {span=> &self, #fn_params }
            };

            Ok(quote_spanned! {span=>
                impl #impl_generics #receiver_type #impl_where_clause {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name #method_generics (#method_args) -> #ret_type #method_where_clause {
                        #func_body
                    }
                }
            })
        }
        // Constructors and static methods share an impl block with no receiver.
        // (Constructor return types may be Result<T, JsValue> for catch constructors.)
        ImportFunctionKind::Constructor { class } | ImportFunctionKind::StaticMethod { class } => {
            let (impl_type, impl_generics, mut method_generics) =
                class_impl_parts(func, class, type_generics);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, true);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name #method_generics (#fn_params) -> #ret_type #method_where_clause {
                        #func_body
                    }
                }
            })
        }
    }
}

fn class_impl_parts(
    func: &ImportFunction,
    class: &str,
    type_generics: &HashMap<String, syn::Generics>,
) -> (syn::Type, syn::Generics, syn::Generics) {
    if type_generics
        .get(class)
        .is_some_and(|generics| !generics.params.is_empty())
        && let Some(class_type) = class_return_type(func, class)
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

fn class_return_type(func: &ImportFunction, class: &str) -> Option<syn::Type> {
    let ret = func
        .ret
        .as_ref()
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

/// Generate code for an async imported function.
fn generate_async_function(
    func: &ImportFunction,
    type_generics: &HashMap<String, syn::Generics>,
    krate: &TokenStream,
    js_code_str: &str,
    args: &GeneratedArgs,
) -> syn::Result<TokenStream> {
    let vis = &func.vis;
    let rust_name = &func.rust_name;
    let span = rust_name.span();
    let rust_attrs = &func.rust_attrs;
    let call_generics = add_js_call_bounds(func, krate, false);
    let (fn_generics, _, fn_where_clause) = call_generics.split_for_impl();

    let fn_params = &args.fn_params;
    let fn_types = &args.fn_type_list;
    let call_values = &args.call_value_list;
    let async_body = quote_spanned! {span=>
        {
            let __wry_promise = #krate::__wry_call_js_function!(
                #js_code_str,
                fn(#(#fn_types),*) -> ::js_sys::Promise,
                (#(#call_values),*)
            );
            ::wasm_bindgen_futures::JsFuture::from(__wry_promise).await
        }
    };

    // Generate return type handling.
    // JsFuture::from(promise).await returns Result<JsValue, JsValue>.
    // - For Result<T, E> return types: map Ok value, keep Err as JsValue
    // - For non-Result types: unwrap and cast
    let (ret_clause, ret_handling) = match &func.ret {
        Some(ty) => {
            // Check if return type is Result<T, E>
            if let Some(ok_type) = extract_result_ok_type(ty) {
                // Return type is Result<T, E> - map the Ok value
                if is_unit_type(&ok_type) {
                    // Result<(), E> - just map to ()
                    (
                        quote_spanned! {span=> -> #ty },
                        quote_spanned! {span=>
                            .map(|_| ())
                        },
                    )
                } else {
                    // Result<T, E> - cast the Ok value
                    (
                        quote_spanned! {span=> -> #ty },
                        quote_spanned! {span=>
                            .map(|v| <#ok_type as #krate::JsCast>::unchecked_from_js(v))
                        },
                    )
                }
            } else {
                // Non-Result type - unwrap and cast
                (
                    quote_spanned! {span=> -> #ty },
                    quote_spanned! {span=>
                        .map(|v| <#ty as #krate::JsCast>::unchecked_from_js(v))
                        .expect("async function failed")
                    },
                )
            }
        }
        None => (
            quote_spanned! {span=> },
            quote_spanned! {span=>
                .expect("async function failed");
            },
        ),
    };

    let allows = clippy_allows();

    // Generate the async function based on kind
    match &func.kind {
        ImportFunctionKind::Normal => Ok(quote_spanned! {span=>
            #allows
            #(#rust_attrs)*
            #vis async fn #rust_name #fn_generics (#fn_params) #ret_clause #fn_where_clause {
                #async_body #ret_handling
            }
        }),
        ImportFunctionKind::Method { receiver }
        | ImportFunctionKind::Getter { receiver, .. }
        | ImportFunctionKind::Setter { receiver, .. }
        | ImportFunctionKind::IndexingGetter { receiver }
        | ImportFunctionKind::IndexingSetter { receiver }
        | ImportFunctionKind::IndexingDeleter { receiver } => {
            // Extract the type name from the receiver
            let receiver_type = receiver_impl_type(receiver)?;
            let (impl_generics, mut method_generics) =
                split_method_generics(&func.generics, receiver);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, false);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();

            // Build method signature with optional additional args
            let method_args = if fn_params.is_empty() {
                quote_spanned! {span=> &self }
            } else {
                quote_spanned! {span=> &self, #fn_params }
            };

            Ok(quote_spanned! {span=>
                impl #impl_generics #receiver_type #impl_where_clause {
                    #allows
                    #(#rust_attrs)*
                    #vis async fn #rust_name #method_generics (#method_args) #ret_clause #method_where_clause {
                        #async_body #ret_handling
                    }
                }
            })
        }
        // Constructors and static methods share an impl block with no receiver.
        ImportFunctionKind::Constructor { class } | ImportFunctionKind::StaticMethod { class } => {
            let (impl_type, impl_generics, mut method_generics) =
                class_impl_parts(func, class, type_generics);
            add_js_call_bounds_to_generics(&mut method_generics, func, krate, false);
            let (impl_generics, _, impl_where_clause) = impl_generics.split_for_impl();
            let (method_generics, _, method_where_clause) = method_generics.split_for_impl();
            Ok(quote_spanned! {span=>
                impl #impl_generics #impl_type #impl_where_clause {
                    #allows
                    #(#rust_attrs)*
                    #vis async fn #rust_name #method_generics (#fn_params) #ret_clause #method_where_clause {
                        #async_body #ret_handling
                    }
                }
            })
        }
    }
}
