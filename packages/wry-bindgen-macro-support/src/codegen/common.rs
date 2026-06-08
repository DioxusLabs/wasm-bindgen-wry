use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, quote_spanned};

pub(super) fn clippy_allows() -> TokenStream {
    quote! {
        #[allow(clippy::unused_unit)]
        #[allow(clippy::too_many_arguments)]
        #[allow(clippy::type_complexity)]
        #[allow(clippy::should_implement_trait)]
        #[allow(clippy::await_holding_refcell_ref)]
    }
}

pub(super) fn generate_wry_call_js_function(
    krate: &TokenStream,
    module: Option<&Ident>,
    js_code: &str,
    fn_type: TokenStream,
    args: TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    match module {
        Some(module) if js_code.contains("{__wry_module}") => quote_spanned! {span=>
            #krate::__wry_call_js_function!(
                module = &#module,
                #js_code,
                #fn_type,
                #args
            )
        },
        _ => quote_spanned! {span=>
            #krate::__wry_call_js_function!(#js_code, #fn_type, #args)
        },
    }
}

pub(super) fn generate_js_reexport_spec(
    static_name: &str,
    export_name: TokenStream,
    namespace: TokenStream,
    module: Option<&Ident>,
    js_code: &str,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let static_ident = format_ident!("{static_name}");
    match module {
        Some(module) if js_code.contains("{__wry_module}") => quote_spanned! {span=>
            const _: () = {
                #[allow(non_upper_case_globals)]
                static #static_ident: #krate::__rt::JsReexportSpec = #krate::__rt::JsReexportSpec::with_module(
                    &#module,
                    #export_name,
                    #namespace,
                    |__wry_module| #krate::__rt::alloc::format!(#js_code, __wry_module = __wry_module),
                );

                #krate::__rt::inventory::submit! {
                    #static_ident
                }
            };
        },
        _ => quote_spanned! {span=>
            const _: () = {
                #[allow(non_upper_case_globals)]
                static #static_ident: #krate::__rt::JsReexportSpec = #krate::__rt::JsReexportSpec::new(
                    #export_name,
                    #namespace,
                    || #krate::__rt::alloc::string::String::from(#js_code),
                );

                #krate::__rt::inventory::submit! {
                    #static_ident
                }
            };
        },
    }
}

pub(super) fn generate_member_type_helpers(
    arg_types: &[TokenStream],
    return_type: Option<TokenStream>,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let return_body = match return_type {
        Some(ty) => quote_spanned! {span=>
            #krate::__rt::core::option::Option::Some(#krate::__rt::TypeDef::of::<#ty>())
        },
        None => quote_spanned! {span=> #krate::__rt::core::option::Option::None },
    };

    quote_spanned! {span=>
        fn __wry_arg_types() -> #krate::__rt::alloc::vec::Vec<#krate::__rt::TypeDef> {
            #krate::__rt::alloc::vec![
                #(#krate::__rt::TypeDef::of::<#arg_types>()),*
            ]
        }

        fn __wry_return_type() -> #krate::__rt::core::option::Option<#krate::__rt::TypeDef> {
            #return_body
        }
    }
}

pub(super) fn generate_js_export_spec(
    static_name: &str,
    export_name: TokenStream,
    body: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let static_ident = format_ident!("{static_name}");
    quote_spanned! {span=>
        const _: () = {
            #[allow(non_upper_case_globals)]
        static #static_ident: #krate::__rt::JsExportSpec = #krate::__rt::JsExportSpec::new(
            #export_name,
            |decoder| {
                let _ = &decoder;
                #body
            }
        );

            #krate::__rt::inventory::submit! {
                #static_ident
            }
        };
    }
}

pub(super) fn namespace_tokens(
    namespace: Option<&[String]>,
    span: proc_macro2::Span,
) -> TokenStream {
    match namespace {
        Some(namespace) if !namespace.is_empty() => {
            let segments: Vec<_> = namespace
                .iter()
                .map(|segment| syn::LitStr::new(segment, span))
                .collect();
            quote_spanned! {span=> &[#(#segments),*] }
        }
        _ => quote_spanned! {span=> &[] },
    }
}

pub(super) struct ClassSpec<'a> {
    pub(super) static_name: &'a str,
    pub(super) class_name: TokenStream,
    pub(super) js_name: TokenStream,
    pub(super) js_namespace: TokenStream,
    pub(super) private: TokenStream,
    pub(super) extends: TokenStream,
    pub(super) extends_js_class: TokenStream,
    pub(super) extends_js_namespace: TokenStream,
    pub(super) inspectable: TokenStream,
    pub(super) public_fields: TokenStream,
}

pub(super) fn generate_js_class_spec(
    spec: ClassSpec<'_>,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let static_ident = format_ident!("{}", spec.static_name);
    let ClassSpec {
        class_name,
        js_name,
        js_namespace,
        private,
        extends,
        extends_js_class,
        extends_js_namespace,
        inspectable,
        public_fields,
        ..
    } = spec;

    quote_spanned! {span=>
        const _: () = {
            #[allow(non_upper_case_globals)]
            static #static_ident: #krate::__rt::JsClassSpec = #krate::__rt::JsClassSpec::new(
                #class_name,
                #js_name,
                #js_namespace,
                #private,
                #extends,
                #extends_js_class,
                #extends_js_namespace,
                #inspectable,
                #public_fields,
            );

            #krate::__rt::inventory::submit! {
                #static_ident
            }
        };
    }
}

pub(super) struct ClassMemberSpec<'a> {
    pub(super) static_name: &'a str,
    pub(super) class_name: TokenStream,
    pub(super) member_name: TokenStream,
    pub(super) export_name: TokenStream,
    pub(super) arg_count: TokenStream,
    pub(super) type_helpers: TokenStream,
    pub(super) member_kind: TokenStream,
    /// `true` when the member takes `self` by value (consuming the receiver).
    pub(super) consumes_self: TokenStream,
}

pub(super) fn generate_js_class_member_spec(
    spec: ClassMemberSpec<'_>,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let static_ident = format_ident!("{}", spec.static_name);
    let ClassMemberSpec {
        class_name,
        member_name,
        export_name,
        arg_count,
        type_helpers,
        member_kind,
        consumes_self,
        ..
    } = spec;

    quote_spanned! {span=>
        const _: () = {
            #type_helpers

            #[allow(non_upper_case_globals)]
            static #static_ident: #krate::__rt::JsClassMemberSpec = #krate::__rt::JsClassMemberSpec::new(
                #class_name,
                #member_name,
                #export_name,
                #arg_count,
                __wry_arg_types,
                __wry_return_type,
                #member_kind,
                #consumes_self,
            );

            #krate::__rt::inventory::submit! {
                #static_ident
            }
        };
    }
}

pub(super) fn generate_js_free_export_spec(
    static_name: &str,
    export_name: TokenStream,
    namespace: TokenStream,
    arg_count: TokenStream,
    arg_names: TokenStream,
    type_helpers: TokenStream,
    this: TokenStream,
    public: TokenStream,
    start: TokenStream,
    variadic: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let static_ident = format_ident!("{}", static_name);

    quote_spanned! {span=>
        const _: () = {
            #type_helpers

            #[allow(non_upper_case_globals)]
            static #static_ident: #krate::__rt::JsFreeExportSpec = #krate::__rt::JsFreeExportSpec::new(
                #export_name,
                #namespace,
                #arg_count,
                #arg_names,
                __wry_arg_types,
                __wry_return_type,
                #this,
                #public,
                #start,
                #variadic,
            );

            #krate::__rt::inventory::submit! {
                #static_ident
            }
        };
    }
}

pub(super) fn extract_result_ok_type(ty: &syn::Type) -> Option<syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident != "Result" {
            return None;
        }
        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments
            && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first()
        {
            return Some(ok_ty.clone());
        }
    }
    None
}
