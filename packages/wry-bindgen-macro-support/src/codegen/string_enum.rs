use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use wasm_bindgen_macro_support::ast::StringEnum;

use super::common::clippy_allows;

pub(super) fn generate_string_enum(
    string_enum: &StringEnum,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let vis = &string_enum.vis;
    let enum_name = &string_enum.name;
    let variants = &string_enum.variants;
    let variant_values = &string_enum.variant_values;
    let rust_attrs = &string_enum.rust_attrs;
    let span = enum_name.span();

    let variant_count = variants.len();
    let variant_indices: Vec<u32> = (0..variant_count as u32).collect();

    let invalid_to_str_msg = format!(
        "Converting an invalid string enum ({enum_name}) back to a string is currently not supported"
    );

    // Generate variant paths for match arms (EnumName::VariantName)
    let variant_paths: Vec<TokenStream> = variants
        .iter()
        .map(|v| quote_spanned!(span=> #enum_name::#v))
        .collect();

    // Generate helper methods (from_str, to_str, from_js_value)
    let allows = clippy_allows();
    let impl_methods = quote! {
        #[automatically_derived]
        impl #enum_name {
            /// Convert a string to this enum variant.
            #allows
            pub fn from_str(s: &str) -> #krate::__rt::core::option::Option<#enum_name> {
                match s {
                    #(#variant_values => #krate::__rt::core::option::Option::Some(#variant_paths),)*
                    _ => #krate::__rt::core::option::Option::None,
                }
            }

            /// Convert this enum variant to its string representation.
            pub fn to_str(&self) -> &'static str {
                match self {
                    #(#variant_paths => #variant_values,)*
                    #enum_name::__Invalid => #krate::__rt::core::panic!(#invalid_to_str_msg),
                }
            }

            /// Convert a JsValue (if it's a string) to this enum variant.
            #allows
            #vis fn from_js_value(obj: &#krate::JsValue) -> #krate::__rt::core::option::Option<#enum_name> {
                #krate::__rt::core::option::Option::and_then(obj.as_string(), |s| Self::from_str(&s))
            }
        }
    };

    // Generate EncodeTypeDef implementation
    // String enums use StringEnum tag with embedded variant strings
    let encode_type_def_impl = quote! {
        impl #krate::__rt::EncodeTypeDef for #enum_name {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                type_def.string_enum(&[#(#variant_values),*]);
            }
        }
    };

    // Generate BinaryEncode implementation - encode as u32 discriminant
    let binary_encode_impl = quote! {
        impl #krate::__rt::BinaryEncode for #enum_name {
            fn encode(self, encoder: &mut #krate::__rt::EncodedData) {
                <u32 as #krate::__rt::BinaryEncode>::encode(self as u32, encoder);
            }
        }
    };

    // Generate BinaryDecode implementation - decode u32 to variant
    let binary_decode_impl = quote! {
        impl #krate::__rt::BinaryDecode for #enum_name {
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self, #krate::__rt::DecodeError> {
                let discriminant = <u32 as #krate::__rt::BinaryDecode>::decode(decoder)?;
                match discriminant {
                    #(#variant_indices => #krate::__rt::core::result::Result::Ok(#variant_paths),)*
                    _ => #krate::__rt::core::result::Result::Ok(#enum_name::__Invalid),
                }
            }
        }
    };

    // Generate BatchableResult implementation
    let batchable_impl = quote! {
        impl #krate::__rt::BatchableResult for #enum_name {}
    };

    // Generate From<EnumName> for JsValue
    let into_jsvalue_impl = quote! {
        #[automatically_derived]
        impl #krate::__rt::core::convert::From<#enum_name> for #krate::JsValue {
            fn from(val: #enum_name) -> Self {
                #krate::JsValue::from_str(val.to_str())
            }
        }
    };

    let try_from_jsvalue_impl = quote! {
        #[automatically_derived]
        impl #krate::convert::TryFromJsValue for #enum_name {
            fn try_from_js_value_ref(value: &#krate::JsValue) -> #krate::__rt::core::option::Option<Self> {
                Self::from_js_value(value)
            }
        }
    };

    let promising_impl = quote! {
        #[automatically_derived]
        impl #krate::sys::Promising for #enum_name {
            type Resolution = #enum_name;
        }
    };

    // Upstream parsing does not preserve string-enum tokens; wasm-bindgen
    // generates a replacement enum with an internal invalid sentinel.
    let enum_def = quote! {
        #(#rust_attrs)*
        #[non_exhaustive]
        #[repr(u32)]
        #vis enum #enum_name {
            #(#variants = #variant_indices,)*
            #[automatically_derived]
            #[doc(hidden)]
            __Invalid
        }
    };

    Ok(quote! {
        #enum_def
        #impl_methods
        #encode_type_def_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_impl
        #into_jsvalue_impl
        #try_from_jsvalue_impl
        #promising_impl
    })
}
