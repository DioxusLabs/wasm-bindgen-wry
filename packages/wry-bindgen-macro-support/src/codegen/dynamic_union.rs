use proc_macro2::TokenStream;
use quote::quote_spanned;
use wasm_bindgen_macro_support::ast::DynamicUnion;

pub(super) fn generate_dynamic_union(
    union: &DynamicUnion,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let vis = &union.vis;
    let enum_name = &union.name;
    let rust_attrs = &union.rust_attrs;
    let span = enum_name.span();
    let invalid_msg = format!("invalid dynamic union value for {enum_name}");

    let _metadata_only = (&union.js_name, union.private);
    let variant_count = union.variants.len();
    let variant_indices: Vec<_> = (0..variant_count)
        .map(|index| syn::LitInt::new(&format!("{index}u8"), span))
        .collect();

    let variant_defs: Vec<_> = union
        .variants
        .iter()
        .zip(&union.variant_fields)
        .map(|(variant, fields)| {
            if fields.is_empty() {
                quote_spanned! {span=> #variant }
            } else {
                let ty = &fields[0];
                quote_spanned! {span=> #variant(#ty) }
            }
        })
        .collect();

    let known_variants: Vec<_> = union
        .variants
        .iter()
        .zip(&union.variant_values)
        .zip(&union.variant_fields)
        .filter_map(|((variant, value), fields)| fields.is_empty().then_some((variant, value)))
        .collect();
    let typed_variants: Vec<_> = union
        .variants
        .iter()
        .zip(&union.variant_fields)
        .filter_map(|(variant, fields)| {
            if fields.is_empty() {
                None
            } else {
                Some((variant, &fields[0]))
            }
        })
        .collect();

    let type_def_variants =
        union
            .variant_values
            .iter()
            .zip(&union.variant_fields)
            .map(|(value, fields)| {
                if fields.is_empty() {
                    quote_spanned! {span=> type_def.dynamic_union_string_variant(#value); }
                } else {
                    let ty = &fields[0];
                    quote_spanned! {span=> type_def.dynamic_union_type_variant::<#ty>(); }
                }
            });

    let known_into_arms = known_variants.iter().map(|(variant, value)| {
        quote_spanned! {span=> #enum_name::#variant => #krate::JsValue::from_str(#value) }
    });
    let typed_into_arms = typed_variants.iter().map(|(variant, _)| {
        quote_spanned! {span=> #enum_name::#variant(__wry_value) => #krate::__rt::core::convert::Into::<#krate::JsValue>::into(__wry_value) }
    });

    let known_from_block = if known_variants.is_empty() {
        TokenStream::new()
    } else {
        let known_from_arms = known_variants.iter().map(|(variant, value)| {
            quote_spanned! {span=> #value => return #krate::__rt::core::result::Result::Ok(#enum_name::#variant), }
        });
        quote_spanned! {span=>
            if let #krate::__rt::core::option::Option::Some(__wry_string) = __wry_value.as_string() {
                match __wry_string.as_str() {
                    #(#known_from_arms)*
                    _ => {}
                }
            }
        }
    };

    let last_fallback_idx = if union.fallback && !typed_variants.is_empty() {
        Some(typed_variants.len() - 1)
    } else {
        None
    };
    let typed_from_arms = typed_variants
        .iter()
        .enumerate()
        .map(|(index, (variant, ty))| {
            if Some(index) == last_fallback_idx {
                quote_spanned! {span=>
                    return #krate::__rt::core::result::Result::Ok(#enum_name::#variant(
                        <#krate::JsValue as #krate::JsCast>::unchecked_into::<#ty>(__wry_value)
                    ));
                }
            } else {
                quote_spanned! {span=>
                    if let #krate::__rt::core::result::Result::Ok(__wry_inner) =
                        <#ty as #krate::convert::TryFromJsValue>::try_from_js_value(__wry_value.clone())
                    {
                        return #krate::__rt::core::result::Result::Ok(#enum_name::#variant(__wry_inner));
                    }
                }
            }
        });
    let from_tail = if last_fallback_idx.is_some() {
        TokenStream::new()
    } else {
        quote_spanned! {span=> #krate::__rt::core::result::Result::Err(__wry_value) }
    };
    let encode_arms = union
        .variants
        .iter()
        .zip(&union.variant_fields)
        .zip(&variant_indices)
        .map(|((variant, fields), index)| {
            if fields.is_empty() {
                quote_spanned! {span=>
                    #enum_name::#variant => {
                        <u8 as #krate::__rt::BinaryEncode>::encode(#index, encoder);
                    }
                }
            } else {
                let ty = &fields[0];
                quote_spanned! {span=>
                    #enum_name::#variant(__wry_value) => {
                        <u8 as #krate::__rt::BinaryEncode>::encode(#index, encoder);
                        <#ty as #krate::__rt::BinaryEncode>::encode(__wry_value, encoder);
                    }
                }
            }
        });

    Ok(quote_spanned! {span=>
        #(#rust_attrs)*
        #vis enum #enum_name {
            #(#variant_defs,)*
        }

        #[automatically_derived]
        impl #enum_name {
            fn __wry_into_js_value(self) -> #krate::JsValue {
                match self {
                    #(#known_into_arms,)*
                    #(#typed_into_arms,)*
                }
            }

            fn __wry_from_js_value(__wry_value: #krate::JsValue) -> #krate::__rt::core::result::Result<Self, #krate::JsValue> {
                #known_from_block
                #(#typed_from_arms)*
                #from_tail
            }
        }

        #[automatically_derived]
        impl #krate::__rt::EncodeTypeDef for #enum_name {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                type_def.dynamic_union(#variant_count, |type_def| {
                    #(#type_def_variants)*
                });
            }
        }

        #[automatically_derived]
        impl #krate::__rt::BinaryEncode for #enum_name {
            fn encode(self, encoder: &mut #krate::__rt::EncodedData) {
                match self {
                    #(#encode_arms,)*
                }
            }
        }

        #[automatically_derived]
        impl #krate::__rt::BinaryDecode for #enum_name {
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self, #krate::__rt::DecodeError> {
                let __wry_value = <#krate::JsValue as #krate::__rt::BinaryDecode>::decode(decoder)?;
                Self::__wry_from_js_value(__wry_value)
                    .map_err(|_| #krate::__rt::DecodeError::custom(#invalid_msg))
            }
        }

        #[automatically_derived]
        impl #krate::__rt::BatchableResult for #enum_name {}

        #[automatically_derived]
        impl #krate::__rt::core::convert::From<#enum_name> for #krate::JsValue {
            fn from(value: #enum_name) -> Self {
                value.__wry_into_js_value()
            }
        }

        #[automatically_derived]
        impl #krate::convert::TryFromJsValue for #enum_name {
            fn try_from_js_value(value: #krate::JsValue) -> #krate::__rt::core::result::Result<Self, #krate::JsValue> {
                Self::__wry_from_js_value(value)
            }

            fn try_from_js_value_ref(value: &#krate::JsValue) -> #krate::__rt::core::option::Option<Self> {
                Self::__wry_from_js_value(value.clone()).ok()
            }
        }

        #[automatically_derived]
        impl #krate::sys::Promising for #enum_name {
            type Resolution = #enum_name;
        }
    })
}
