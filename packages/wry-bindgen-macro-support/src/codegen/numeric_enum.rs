use proc_macro2::TokenStream;
use quote::quote_spanned;
use wasm_bindgen_macro_support::ast::Enum;

use super::common::{generate_js_reexport_spec, namespace_tokens};

fn value_tokens(value: i64, span: proc_macro2::Span) -> TokenStream {
    if value < 0 {
        let abs = syn::LitInt::new(&value.abs().to_string(), span);
        quote_spanned! {span=> -#abs }
    } else {
        let value = syn::LitInt::new(&value.to_string(), span);
        quote_spanned! {span=> #value }
    }
}

fn variant_value(value: u32, signed: bool) -> i64 {
    if signed {
        value as i32 as i64
    } else {
        value as i64
    }
}

fn js_enum_object_expr(e: &Enum) -> String {
    let mut out = String::from("(() => { const e = {}; ");
    for variant in &e.variants {
        let name = &variant.js_name;
        let value = variant_value(variant.value, e.signed);
        out.push_str(&format!("e[{name:?}] = {value}; e[{value:?}] = {name:?}; "));
    }
    out.push_str("return e; })()");
    out
}

pub(super) fn generate_numeric_enum(e: &Enum, krate: &TokenStream) -> syn::Result<TokenStream> {
    let enum_name = &e.rust_name;
    let variants: Vec<_> = e
        .variants
        .iter()
        .map(|variant| &variant.rust_name)
        .collect();
    let values: Vec<_> = e
        .variants
        .iter()
        .map(|variant| value_tokens(variant_value(variant.value, e.signed), enum_name.span()))
        .collect();
    let backing_ty = if e.signed {
        quote_spanned! {enum_name.span()=> i32 }
    } else {
        quote_spanned! {enum_name.span()=> u32 }
    };
    let invalid_msg = format!("invalid value for enum {enum_name}");
    let js_name = &e.js_name;
    let enum_object = if e.private {
        TokenStream::new()
    } else {
        generate_js_reexport_spec(
            "__NUMERIC_ENUM_SPEC",
            quote_spanned! {enum_name.span()=> #js_name },
            namespace_tokens(e.js_namespace.as_deref(), enum_name.span()),
            None,
            &js_enum_object_expr(e),
            krate,
            enum_name.span(),
        )
    };

    Ok(quote_spanned! {enum_name.span()=>
        impl #krate::__rt::EncodeTypeDef for #enum_name {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                <#backing_ty as #krate::__rt::EncodeTypeDef>::encode_type_def(type_def);
            }
        }

        impl #krate::__rt::BinaryEncode for #enum_name {
            fn encode(self, encoder: &mut #krate::__rt::EncodedData) {
                <#backing_ty as #krate::__rt::BinaryEncode>::encode(self as #backing_ty, encoder);
            }
        }

        impl #krate::__rt::BinaryDecode for #enum_name {
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self, #krate::__rt::DecodeError> {
                match <#backing_ty as #krate::__rt::BinaryDecode>::decode(decoder)? {
                    #(#values => #krate::__rt::core::result::Result::Ok(#enum_name::#variants),)*
                    _ => #krate::__rt::core::result::Result::Err(#krate::__rt::DecodeError::custom(#invalid_msg)),
                }
            }
        }

        impl #krate::__rt::BatchableResult for #enum_name {}

        impl #krate::__rt::core::convert::From<#enum_name> for #krate::JsValue {
            fn from(value: #enum_name) -> Self {
                #krate::JsValue::from(value as #backing_ty)
            }
        }

        impl #krate::convert::TryFromJsValue for #enum_name {
            fn try_from_js_value_ref(value: &#krate::JsValue) -> #krate::__rt::core::option::Option<Self> {
                match <#backing_ty as #krate::convert::TryFromJsValue>::try_from_js_value_ref(value)? {
                    #(#values => #krate::__rt::core::option::Option::Some(#enum_name::#variants),)*
                    _ => #krate::__rt::core::option::Option::None,
                }
            }
        }

        impl #krate::sys::Promising for #enum_name {
            type Resolution = #enum_name;
        }

        #enum_object
    })
}
