use crate::ast::ImportType;
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote, quote_spanned};

use super::common::generate_wry_call_js_function;
use super::erasure::add_static_bounds;

pub(super) fn generate_type(
    ty: &ImportType,
    krate: &TokenStream,
    module: Option<&Ident>,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &ty.vis;
    let rust_name = &ty.rust_name;
    let generics = &ty.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut into_js_generics = add_static_bounds(generics);
    let (_, into_js_ty_generics, _) = into_js_generics.split_for_impl();
    let self_ty: syn::Type = syn::parse_quote!(#rust_name #into_js_ty_generics);
    into_js_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(#self_ty: #krate::JsGeneric));
    let (into_js_impl_generics, into_js_ty_generics, into_js_where_clause) =
        into_js_generics.split_for_impl();
    let derives = &ty.derives;
    let span = rust_name.span();
    let storage_ty = if let Some(first_parent) = ty.extends.first() {
        first_parent.to_token_stream()
    } else {
        quote_spanned! {span=> #krate::JsValue }
    };
    let type_params: Vec<_> = generics.type_params().map(|param| &param.ident).collect();
    let js_option_inner = if rust_name == "JsOption"
        && ty.js_name == "JsOption"
        && ty.no_upcast
        && ty.extends.is_empty()
        && type_params.len() == 1
    {
        type_params.first().copied()
    } else {
        None
    };
    let generic_field = if type_params.is_empty() {
        quote! {}
    } else {
        quote_spanned! {span=>
            #[doc(hidden)]
            pub generics: ::core::marker::PhantomData<fn() -> (#(#type_params,)*)>,
        }
    };
    let generic_init = if type_params.is_empty() {
        quote! {}
    } else {
        quote_spanned! {span=>
            generics: ::core::marker::PhantomData,
        }
    };
    let from_jsvalue_obj = if ty.extends.is_empty() {
        quote_spanned! {span=> val }
    } else {
        quote_spanned! {span=> <#storage_ty as #krate::JsCast>::unchecked_from_js(val) }
    };

    // Generate the struct definition using JsValue from the configured crate
    // repr(transparent) ensures the same memory layout
    // Apply user-provided attributes (like #[derive(Debug, PartialEq, Eq)])
    // Use named struct with `obj` field to match wasm-bindgen's generated types
    let struct_def = quote_spanned! {span=>
        #(#derives)*
        #[repr(transparent)]
        #vis struct #rust_name #generics #where_clause {
            #[doc(hidden)]
            pub obj: #storage_ty,
            #generic_field
        }
    };

    // Generate AsRef<JsValue> implementation
    let as_ref_impl = quote_spanned! {span=>
        impl #impl_generics ::core::convert::AsRef<#krate::JsValue> for #rust_name #ty_generics #where_clause {
            fn as_ref(&self) -> &#krate::JsValue {
                ::core::convert::AsRef::as_ref(&self.obj)
            }
        }
    };

    // Generate From<Type> for JsValue and From<JsValue> for Type
    let into_jsvalue = quote_spanned! {span=>
        impl #impl_generics ::core::convert::From<#rust_name #ty_generics> for #krate::JsValue #where_clause {
            fn from(val: #rust_name #ty_generics) -> Self {
                ::core::convert::Into::into(val.obj)
            }
        }

        impl #impl_generics ::core::convert::From<#krate::JsValue> for #rust_name #ty_generics #where_clause {
            fn from(val: #krate::JsValue) -> Self {
                Self { obj: #from_jsvalue_obj, #generic_init }
            }
        }
    };

    // Generate Deref to the first parent or JsValue if no parents
    let deref_impls = {
        if let Some(inner) = js_option_inner {
            quote_spanned! {span=>
                impl<#inner: #krate::JsGeneric> ::core::ops::Deref for #rust_name<#inner> {
                    type Target = #inner;
                    fn deref(&self) -> &#inner {
                        <#inner as #krate::JsCast>::unchecked_from_js_ref(&self.obj)
                    }
                }
            }
        } else {
            let deref_to = &storage_ty;
            quote_spanned! {span=>
                impl #impl_generics ::core::ops::Deref for #rust_name #ty_generics #where_clause {
                    type Target = #deref_to;
                    fn deref(&self) -> &#deref_to {
                        <Self as ::core::convert::AsRef<#deref_to>>::as_ref(self)
                    }
                }
            }
        }
    };

    // Generate owned From and borrowed AsRef impls for parent types.
    //
    // Keep this aligned with upstream wasm-bindgen: a borrowed upcast should
    // stay borrowed. Emitting From<&Child> for Parent forces the parent wrapper
    // to implement Clone, which plain extern types do not do by default.
    let mut from_parents = TokenStream::new();
    from_parents.extend(quote_spanned! {span=>
        impl #impl_generics ::core::convert::AsRef<#rust_name #ty_generics> for #rust_name #ty_generics #where_clause {
            #[inline]
            fn as_ref(&self) -> &#rust_name #ty_generics {
                self
            }
        }
    });
    for (index, parent) in ty.extends.iter().enumerate() {
        let parent_from_owned = if index == 0 {
            quote_spanned! {span=> val.obj }
        } else {
            quote_spanned! {span=> <#parent as #krate::JsCast>::unchecked_from_js(::core::convert::Into::into(val.obj)) }
        };
        let parent_ref = if index == 0 {
            quote_spanned! {span=> &self.obj }
        } else {
            quote_spanned! {span=> <#parent as #krate::JsCast>::unchecked_from_js_ref(::core::convert::AsRef::<#krate::JsValue>::as_ref(self)) }
        };
        from_parents.extend(quote_spanned! {span=>
            impl #impl_generics ::core::convert::From<#rust_name #ty_generics> for #parent #where_clause {
                fn from(val: #rust_name #ty_generics) -> #parent {
                    #parent_from_owned
                }
            }

            impl #impl_generics ::core::convert::AsRef<#parent> for #rust_name #ty_generics #where_clause {
                #[inline]
                fn as_ref(&self) -> &#parent {
                    #parent_ref
                }
            }
        });
    }

    // Generate EncodeTypeDef implementation
    // All JS types use HeapRef since they're references to JS heap objects
    let encode_type_def_impl = quote_spanned! {span=>
        impl #impl_generics #krate::__rt::EncodeTypeDef for #rust_name #ty_generics #where_clause {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                <#krate::JsValue as #krate::__rt::EncodeTypeDef>::encode_type_def(type_def);
            }
        }
    };

    // Generate BinaryEncode implementation
    let binary_encode_impl = quote_spanned! {span=>
        impl #impl_generics #krate::__rt::BinaryEncode for #rust_name #ty_generics #where_clause {
            fn encode(self, encoder: &mut #krate::__rt::EncodedData) {
                self.obj.encode(encoder);
            }
        }
    };

    let js_ref_encode_impl = quote_spanned! {span=>
        impl #impl_generics #krate::__rt::JsRefEncode for #rust_name #ty_generics #where_clause {
            fn js_ref(&self) -> #krate::__rt::JsRef {
                self.obj.js_ref()
            }
        }

    };

    // Generate BinaryDecode implementation
    let binary_decode_impl = quote_spanned! {span=>
        impl #impl_generics #krate::__rt::BinaryDecode for #rust_name #ty_generics #where_clause {
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> ::core::result::Result<Self, #krate::__rt::DecodeError> {
                ::core::result::Result::map(#krate::JsValue::decode(decoder), ::core::convert::Into::into)
            }
        }
    };

    // Generate BatchableResult implementation
    let batchable_impl = quote_spanned! {span=>
        impl #impl_generics #krate::__rt::BatchableResult for #rust_name #ty_generics #where_clause {
            fn try_placeholder(batch: &mut #krate::__rt::Runtime) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(::core::convert::Into::into(<#krate::JsValue as #krate::__rt::BatchableResult>::try_placeholder(batch)?))
            }
        }
    };

    // Generate JsCast implementation with actual instanceof check
    let js_name = &ty.js_name;

    // Generate JavaScript instanceof check code with vendor prefix fallback
    // Always generate a safe check that returns false if the class doesn't exist,
    // matching wasm-bindgen's try-catch behavior
    let instanceof_js_code = if ty.vendor_prefixes.is_empty() {
        // Simple case: check if class exists before instanceof
        format!(
            "(a0) => typeof {prefix}{js_name} !== 'undefined' && a0 instanceof {prefix}{js_name}"
        )
    } else {
        // Generate vendor-prefixed fallback:
        // (a0) => a0 instanceof (typeof Foo !== 'undefined' ? Foo : (typeof webkitFoo !== 'undefined' ? webkitFoo : ...))
        let mut class_expr =
            format!("(typeof {prefix}{js_name} !== 'undefined' ? {prefix}{js_name} : ");
        for (i, vendor_prefix) in ty.vendor_prefixes.iter().enumerate() {
            let prefixed = format!("{vendor_prefix}{js_name}");
            if i == ty.vendor_prefixes.len() - 1 {
                // Last prefix - use Object as final fallback (which will make instanceof return false for non-objects)
                class_expr.push_str(&format!(
                    "(typeof {prefix}{prefixed} !== 'undefined' ? {prefix}{prefixed} : Object)"
                ));
            } else {
                class_expr.push_str(&format!(
                    "(typeof {prefix}{prefixed} !== 'undefined' ? {prefix}{prefixed} : "
                ));
            }
        }
        // Close all the parentheses
        class_expr.push(')');
        format!("(a0) => a0 instanceof {class_expr}")
    };

    // Generate is_type_of implementation if provided
    let is_type_of_impl = ty.is_type_of.as_ref().map(|is_type_of| {
        quote_spanned! {span=>
            #[inline]
            fn is_type_of(__val: &#krate::JsValue) -> bool {
                let __is_type_of: fn(&#krate::JsValue) -> bool = #is_type_of;
                __is_type_of(__val)
            }
        }
    });
    let instanceof_call = generate_wry_call_js_function(
        krate,
        module,
        &instanceof_js_code,
        quote_spanned! {span=> fn(&#krate::JsValue) -> bool },
        quote_spanned! {span=> (__val) },
        span,
    );

    let jscast_impl = if let Some(inner) = js_option_inner {
        quote_spanned! {span=>
            impl<#inner: #krate::JsGeneric> #krate::JsCast for #rust_name<#inner> {
                fn instanceof(__val: &#krate::JsValue) -> bool {
                    <#inner as #krate::JsCast>::is_type_of(__val)
                        || __val.is_null()
                        || __val.is_undefined()
                }

                fn unchecked_from_js(val: #krate::JsValue) -> Self {
                    ::core::convert::Into::into(val)
                }

                fn unchecked_from_js_ref(val: &#krate::JsValue) -> &Self {
                    // SAFETY: #[repr(transparent)] guarantees same layout
                    unsafe { &*(val as *const #krate::JsValue as *const Self) }
                }
            }
        }
    } else {
        quote_spanned! {span=>
            impl #impl_generics #krate::JsCast for #rust_name #ty_generics #where_clause {
                fn instanceof(__val: &#krate::JsValue) -> bool {
                    #instanceof_call
                }

                #is_type_of_impl

                fn unchecked_from_js(val: #krate::JsValue) -> Self {
                    ::core::convert::Into::into(val)
                }

                fn unchecked_from_js_ref(val: &#krate::JsValue) -> &Self {
                    // SAFETY: #[repr(transparent)] guarantees same layout
                    unsafe { &*(val as *const #krate::JsValue as *const Self) }
                }
            }
        }
    };

    let generic_trait_impls = quote_spanned! {span=>
        unsafe impl #impl_generics #krate::__rt::marker::ErasableGeneric for #rust_name #ty_generics #where_clause {
            type Repr = #krate::JsValue;
        }

        impl #into_js_impl_generics #krate::IntoJsGeneric for #rust_name #into_js_ty_generics #into_js_where_clause {
            type JsCanon = Self;

            #[inline]
            fn to_js(self) -> Self::JsCanon {
                self
            }
        }

    };

    let promising_impl = if ty.no_promising {
        quote! {}
    } else {
        quote_spanned! {span=>
            impl #impl_generics #krate::sys::Promising for #rust_name #ty_generics #where_clause {
                type Resolution = #rust_name #ty_generics;
            }
        }
    };

    let mut upcast_impls = TokenStream::new();
    if !ty.no_upcast {
        upcast_impls.extend(quote_spanned! {span=>
            impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #krate::JsValue #where_clause {}
            impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #krate::sys::JsOption<#krate::JsValue> #where_clause {}
        });

        let class_type_params: Vec<_> = generics.type_params().collect();
        if class_type_params.is_empty() {
            upcast_impls.extend(quote_spanned! {span=>
                impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #rust_name #ty_generics #where_clause {}
                impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #krate::sys::JsOption<#rust_name #ty_generics> #where_clause {}
            });
        } else {
            let mut target_generics = generics.clone();
            let target_param_names: Vec<_> = class_type_params
                .iter()
                .enumerate()
                .map(|(index, param)| {
                    let target_name = format_ident!("__WryUpcastTarget{}", index);
                    let bounds = &param.bounds;
                    if bounds.is_empty() {
                        target_generics.params.push(syn::parse_quote!(#target_name));
                    } else {
                        target_generics
                            .params
                            .push(syn::parse_quote!(#target_name: #bounds));
                    }
                    target_name
                })
                .collect();
            let mut target_where_clause =
                generics
                    .where_clause
                    .clone()
                    .unwrap_or_else(|| syn::WhereClause {
                        where_token: Default::default(),
                        predicates: Default::default(),
                    });
            for (param, target_name) in class_type_params.iter().zip(&target_param_names) {
                let param_name = &param.ident;
                target_where_clause.predicates.push(syn::parse_quote!(
                    #target_name: #krate::convert::UpcastFrom<#param_name>
                ));
            }
            let (target_impl_generics, _, _) = target_generics.split_for_impl();
            let mut target_args = Vec::new();
            let mut next_type_param = 0usize;
            for param in &generics.params {
                match param {
                    syn::GenericParam::Lifetime(param) => {
                        let lifetime = &param.lifetime;
                        target_args.push(quote! { #lifetime });
                    }
                    syn::GenericParam::Type(_) => {
                        let target_name = &target_param_names[next_type_param];
                        next_type_param += 1;
                        target_args.push(quote! { #target_name });
                    }
                    syn::GenericParam::Const(param) => {
                        let ident = &param.ident;
                        target_args.push(quote! { #ident });
                    }
                }
            }
            let target_ty_generics = if target_args.is_empty() {
                quote! {}
            } else {
                quote! { <#(#target_args),*> }
            };

            upcast_impls.extend(quote_spanned! {span=>
                impl #target_impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #rust_name #target_ty_generics #target_where_clause {}
                impl #target_impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #krate::sys::JsOption<#rust_name #target_ty_generics> #target_where_clause {}
            });
        }

        for parent in &ty.extends {
            upcast_impls.extend(quote_spanned! {span=>
                impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #parent #where_clause {}
                impl #impl_generics #krate::convert::UpcastFrom<#rust_name #ty_generics> for #krate::sys::JsOption<#parent> #where_clause {}
            });
        }
    }

    Ok(quote_spanned! {span=>
        #struct_def
        #as_ref_impl
        #into_jsvalue
        #deref_impls
        #from_parents
        #encode_type_def_impl
        #binary_encode_impl
        #js_ref_encode_impl
        #binary_decode_impl
        #batchable_impl
        #jscast_impl
        #generic_trait_impls
        #promising_impl
        #upcast_impls
    })
}
