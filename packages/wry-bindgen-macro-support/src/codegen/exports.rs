use crate::ast::{
    ExportFunction, ExportMethod, ExportMethodKind, ExportStruct, SelfType, StructField,
};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use super::common::{
    ClassMemberSpec, FreeFunctionSpec, clippy_allows, generate_js_class_member_spec,
    generate_js_export_spec, generate_js_free_function_spec, generate_member_type_helpers,
};

pub(super) fn generate_export_struct(
    s: &ExportStruct,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let vis = &s.vis;
    let rust_name = &s.rust_name;
    let js_name = &s.js_name;
    let rust_attrs = &s.rust_attrs;
    let span = rust_name.span();

    // Generate field definitions for the struct
    let field_defs: Vec<_> = s
        .fields
        .iter()
        .map(|f| {
            let field_vis = &f.vis;
            let field_name = &f.rust_name;
            let field_ty = &f.ty;
            quote_spanned! {span=> #field_vis #field_name: #field_ty }
        })
        .collect();

    // Generate the struct itself
    let struct_def = quote_spanned! {span=>
        #(#rust_attrs)*
        #vis struct #rust_name {
            #(#field_defs),*
        }
    };

    // Generate field getters and setters
    let mut field_impls = TokenStream::new();
    for field in &s.fields {
        field_impls.extend(generate_field_accessor(rust_name, field, krate)?);
    }

    // Generate drop function
    let drop_fn_name = format!("{js_name}::__drop");
    let drop_impl = generate_js_export_spec(
        "__DROP_SPEC",
        quote_spanned! {span=> #drop_fn_name },
        quote_spanned! {span=>
            let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(
                decoder
            )?;
            #krate::__rt::object_store::drop_object(handle);
            Ok(#krate::__rt::EncodedData::default())
        },
        krate,
        span,
    );

    // Generate inspectable methods if enabled
    let inspectable_impl = if s.is_inspectable {
        generate_inspectable(rust_name, &s.fields, js_name, krate)?
    } else {
        TokenStream::new()
    };

    // Generate From<StructName> for JsValue - inserts into object store and returns handle
    let into_jsvalue_impl = quote_spanned! {span=>
        impl ::core::convert::From<#rust_name> for #krate::JsValue {
            fn from(val: #rust_name) -> Self {
                let handle = #krate::__rt::object_store::insert_object(val);
                // Create a JS object wrapper with the handle
                #krate::__rt::object_store::create_js_wrapper(handle, #js_name)
            }
        }
    };

    // Generate EncodeTypeDef - exported structs use HeapRef encoding
    let encode_type_def_impl = quote_spanned! {span=>
        impl #krate::__rt::EncodeTypeDef for #rust_name {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                <#krate::JsValue as #krate::__rt::EncodeTypeDef>::encode_type_def(type_def);
            }
        }
    };

    // Generate BinaryEncode - encode struct by converting to JsValue
    let binary_encode_impl = quote_spanned! {span=>
        impl #krate::__rt::BinaryEncode for #rust_name {
            fn encode(self, encoder: &mut #krate::__rt::EncodedData) {
                // Convert to JsValue (which inserts into object store and creates wrapper)
                let js_value = #krate::JsValue::from(self);
                // Encode the JsValue
                js_value.encode(encoder);
            }
        }
    };

    // Generate BinaryDecode - decode JsValue, extract handle, remove from object store
    let binary_decode_impl = quote_spanned! {span=>
        impl #krate::__rt::BinaryDecode for #rust_name {
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> ::core::result::Result<Self, #krate::__rt::DecodeError> {
                // Decode the JsValue
                let js = #krate::JsValue::decode(decoder)?;
                // Extract handle from JS wrapper
                let handle = #krate::__rt::extract_rust_handle(&js)
                    .ok_or_else(|| #krate::__rt::DecodeError::custom(
                        #krate::alloc::string::String::from("expected Rust object wrapper")
                    ))?;
                // Remove from object store and return owned value
                Ok(#krate::__rt::object_store::remove_object::<#rust_name>(handle))
            }
        }
    };

    // Generate BatchableResult - exported structs need flush to get actual value
    let batchable_result_impl = quote_spanned! {span=>
        impl #krate::__rt::BatchableResult for #rust_name {}
    };

    Ok(quote_spanned! {span=>
        #struct_def
        #field_impls
        #drop_impl
        #inspectable_impl
        #into_jsvalue_impl
        #encode_type_def_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_result_impl
    })
}

/// Generate getter and setter for a struct field
fn generate_field_accessor(
    struct_name: &syn::Ident,
    field: &StructField,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let field_name = &field.rust_name;
    let js_field_name = &field.js_name;
    let field_ty = &field.ty;
    let span = field_name.span();

    // Only generate accessors for public fields
    if !matches!(field.vis, syn::Visibility::Public(_)) {
        return Ok(TokenStream::new());
    }

    let struct_name_str = struct_name.to_string();
    let getter_name = format!("{struct_name_str}::{js_field_name}_get");
    let setter_name = format!("{struct_name_str}::{js_field_name}_set");

    // Generate getter
    let getter_body = if field.getter_with_clone {
        quote_spanned! {span=>
            #krate::__rt::object_store::with_object::<#struct_name, _>(handle, |obj| {
                let val = ::core::clone::Clone::clone(&obj.#field_name);
                let mut encoder = #krate::__rt::EncodedData::default();
                <#field_ty as #krate::__rt::BinaryEncode>::encode(val, &mut encoder);
                Ok(encoder)
            })
        }
    } else {
        quote_spanned! {span=>
            #krate::__rt::object_store::with_object::<#struct_name, _>(handle, |obj| {
                let val = obj.#field_name;
                let mut encoder = #krate::__rt::EncodedData::default();
                <#field_ty as #krate::__rt::BinaryEncode>::encode(val, &mut encoder);
                Ok(encoder)
            })
        }
    };

    let getter_impl = generate_js_export_spec(
        "__GETTER_SPEC",
        quote_spanned! {span=> #getter_name },
        quote_spanned! {span=>
            let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
            #getter_body
        },
        krate,
        span,
    );

    // Generate setter (unless readonly)
    let setter_impl = if !field.readonly {
        generate_js_export_spec(
            "__SETTER_SPEC",
            quote_spanned! {span=> #setter_name },
            quote_spanned! {span=>
                let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                let val = <#field_ty as #krate::__rt::BinaryDecode>::decode(decoder)?;
                #krate::__rt::object_store::with_object_mut::<#struct_name, _>(handle, |obj| {
                    obj.#field_name = val;
                });
                Ok(#krate::__rt::EncodedData::default())
            },
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    // Generate JsClassMemberSpec for the property getter
    let js_class_name = struct_name.to_string();
    let getter_type_helpers =
        generate_member_type_helpers(&[], Some(quote_spanned! {span=> #field_ty }), krate, span);
    let getter_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__GETTER_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #js_class_name },
            member_name: quote_spanned! {span=> #js_field_name },
            export_name: quote_spanned! {span=> #getter_name },
            arg_count: quote_spanned! {span=> 0 },
            type_helpers: getter_type_helpers,
            member_kind: quote_spanned! {span=> #krate::__rt::JsClassMemberKind::Getter },
        },
        krate,
        span,
    );

    // Generate JsClassMemberSpec for the property setter (unless readonly)
    let setter_member_spec = if !field.readonly {
        let setter_arg_types = vec![quote_spanned! {span=> #field_ty }];
        let setter_type_helpers =
            generate_member_type_helpers(&setter_arg_types, None, krate, span);
        generate_js_class_member_spec(
            ClassMemberSpec {
                static_name: "__SETTER_MEMBER_SPEC",
                class_name: quote_spanned! {span=> #js_class_name },
                member_name: quote_spanned! {span=> #js_field_name },
                export_name: quote_spanned! {span=> #setter_name },
                arg_count: quote_spanned! {span=> 1 },
                type_helpers: setter_type_helpers,
                member_kind: quote_spanned! {span=> #krate::__rt::JsClassMemberKind::Setter },
            },
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    Ok(quote_spanned! {span=>
        #getter_impl
        #setter_impl
        #getter_member_spec
        #setter_member_spec
    })
}

/// Generate toJSON and toString methods for inspectable structs
fn generate_inspectable(
    struct_name: &syn::Ident,
    fields: &[StructField],
    js_name: &str,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let span = struct_name.span();
    let to_json_name = format!("{js_name}::toJSON");
    let to_string_name = format!("{js_name}::toString");

    // Build JSON object from fields
    let field_names: Vec<_> = fields
        .iter()
        .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
        .map(|f| &f.js_name)
        .collect();
    let field_idents: Vec<_> = fields
        .iter()
        .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
        .map(|f| &f.rust_name)
        .collect();

    let struct_name_str = struct_name.to_string();

    let js_name_str = js_name.to_string();
    let string_return_type = Some(quote_spanned! {span=> ::alloc::string::String });
    let to_json_type_helpers =
        generate_member_type_helpers(&[], string_return_type.clone(), krate, span);
    let to_string_type_helpers = generate_member_type_helpers(&[], string_return_type, krate, span);

    let to_json_export_spec = generate_js_export_spec(
        "__TO_JSON_SPEC",
        quote_spanned! {span=> #to_json_name },
        quote_spanned! {span=>
            let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
            #krate::__rt::object_store::with_object::<#struct_name, _>(handle, |obj| {
                // Create a simple JSON-like representation
                let mut json = ::alloc::string::String::from("{");
                #(
                    json.push_str(&::alloc::format!("\"{}\":{:?},", #field_names, obj.#field_idents));
                )*
                if json.ends_with(',') {
                    json.pop();
                }
                json.push('}');
                let mut encoder = #krate::__rt::EncodedData::default();
                <::alloc::string::String as #krate::__rt::BinaryEncode>::encode(json, &mut encoder);
                Ok(encoder)
            })
        },
        krate,
        span,
    );
    let to_json_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__TO_JSON_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #js_name_str },
            member_name: quote_spanned! {span=> "toJSON" },
            export_name: quote_spanned! {span=> #to_json_name },
            arg_count: quote_spanned! {span=> 0 },
            type_helpers: to_json_type_helpers,
            member_kind: quote_spanned! {span=> #krate::__rt::JsClassMemberKind::Method },
        },
        krate,
        span,
    );
    let to_string_export_spec = generate_js_export_spec(
        "__TO_STRING_SPEC",
        quote_spanned! {span=> #to_string_name },
        quote_spanned! {span=>
            let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
            #krate::__rt::object_store::with_object::<#struct_name, _>(handle, |obj| {
                let s = ::alloc::format!("[object {}]", #struct_name_str);
                let mut encoder = #krate::__rt::EncodedData::default();
                <::alloc::string::String as #krate::__rt::BinaryEncode>::encode(s, &mut encoder);
                Ok(encoder)
            })
        },
        krate,
        span,
    );
    let to_string_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__TO_STRING_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #js_name_str },
            member_name: quote_spanned! {span=> "toString" },
            export_name: quote_spanned! {span=> #to_string_name },
            arg_count: quote_spanned! {span=> 0 },
            type_helpers: to_string_type_helpers,
            member_kind: quote_spanned! {span=> #krate::__rt::JsClassMemberKind::Method },
        },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #to_json_export_spec
        #to_json_member_spec
        #to_string_export_spec
        #to_string_member_spec
    })
}

/// Generate code for an exported free function.
pub(super) fn generate_export_function(
    function: &ExportFunction,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let rust_name = &function.rust_name;
    let js_name = &function.js_name;
    let span = rust_name.span();
    let export_name = js_name.clone();

    let arg_names: Vec<_> = function.arguments.iter().map(|a| &a.name).collect();
    let arg_types: Vec<_> = function.arguments.iter().map(|a| &a.ty).collect();

    let decode_args = quote_spanned! {span=>
        #(
            let #arg_names = <#arg_types as #krate::__rt::BinaryDecode>::decode(decoder)?;
        )*
    };

    let function_body = if let Some(ret_ty) = &function.ret {
        quote_spanned! {span=>
            #decode_args
            let result = #rust_name(#(#arg_names),*);
            let mut encoder = #krate::__rt::EncodedData::default();
            <#ret_ty as #krate::__rt::BinaryEncode>::encode(result, &mut encoder);
            Ok(encoder)
        }
    } else {
        quote_spanned! {span=>
            #decode_args
            #rust_name(#(#arg_names),*);
            Ok(#krate::__rt::EncodedData::default())
        }
    };

    let vis = &function.vis;
    let body = &function.body;
    let rust_attrs = function.fn_rust_attrs();
    let fn_args: Vec<_> = arg_names
        .iter()
        .zip(arg_types.iter())
        .map(|(name, ty)| quote_spanned! {span=> #name: #ty })
        .collect();
    let ret_type = match &function.ret {
        Some(ty) => quote_spanned! {span=> -> #ty },
        None => quote_spanned! {span=> },
    };

    let allows = clippy_allows();
    let function_def = quote_spanned! {span=>
        #allows
        #rust_attrs
        #vis fn #rust_name(#(#fn_args),*) #ret_type #body
    };

    let export_spec = generate_js_export_spec(
        "__EXPORT_SPEC",
        quote_spanned! {span=> #export_name },
        quote_spanned! {span=>
            #function_body
        },
        krate,
        span,
    );

    let arg_count = function.arguments.len();
    let free_arg_types: Vec<TokenStream> = function
        .arguments
        .iter()
        .map(|arg| {
            let ty = &arg.ty;
            quote_spanned! {span=> #ty }
        })
        .collect();
    let free_return_type = function.ret.as_ref().map(|ty| quote_spanned! {span=> #ty });
    let free_type_helpers =
        generate_member_type_helpers(&free_arg_types, free_return_type, krate, span);
    let free_function_spec = generate_js_free_function_spec(
        FreeFunctionSpec {
            static_name: "__FREE_FUNCTION_SPEC",
            js_name: quote_spanned! {span=> #js_name },
            export_name: quote_spanned! {span=> #export_name },
            arg_count: quote_spanned! {span=> #arg_count },
            type_helpers: free_type_helpers,
        },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #function_def
        #export_spec
        #free_function_spec
    })
}

/// Generate code for an exported method
pub(super) fn generate_export_method(
    method: &ExportMethod,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let class = &method.class;
    let rust_name = &method.rust_name;
    let js_name = &method.js_name;
    let span = rust_name.span();

    let class_str = class.to_string();
    let export_name = format!("{class_str}::{js_name}");

    // Generate argument decoding
    let arg_names: Vec<_> = method.arguments.iter().map(|a| &a.name).collect();
    let arg_types: Vec<_> = method.arguments.iter().map(|a| &a.ty).collect();

    let decode_args = quote_spanned! {span=>
        #(
            let #arg_names = <#arg_types as #krate::__rt::BinaryDecode>::decode(decoder)?;
        )*
    };

    // Generate the method call and return encoding based on kind
    let method_body = match &method.kind {
        ExportMethodKind::Constructor => {
            // Constructor: create new instance and store in object store
            quote_spanned! {span=>
                #decode_args
                let result = #class::#rust_name(#(#arg_names),*);
                let handle = #krate::__rt::object_store::insert_object(result);
                let mut encoder = #krate::__rt::EncodedData::default();
                <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryEncode>::encode(handle, &mut encoder);
                Ok(encoder)
            }
        }
        ExportMethodKind::Method { self_ty } => {
            // Instance method: get object from store, call method
            let call = match self_ty {
                SelfType::RefShared => {
                    quote_spanned! {span=>
                        #krate::__rt::object_store::with_object::<#class, _>(handle, |obj| {
                            obj.#rust_name(#(#arg_names),*)
                        })
                    }
                }
                SelfType::RefMutable => {
                    quote_spanned! {span=>
                        #krate::__rt::object_store::with_object_mut::<#class, _>(handle, |obj| {
                            obj.#rust_name(#(#arg_names),*)
                        })
                    }
                }
                SelfType::ByValue => {
                    // Consuming method: remove from store
                    quote_spanned! {span=>
                        {
                            let obj = #krate::__rt::object_store::remove_object::<#class>(handle);
                            obj.#rust_name(#(#arg_names),*)
                        }
                    }
                }
            };

            if let Some(ret_ty) = method.ret.as_ref() {
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #decode_args
                    let result = #call;
                    let mut encoder = #krate::__rt::EncodedData::default();
                    <#ret_ty as #krate::__rt::BinaryEncode>::encode(result, &mut encoder);
                    Ok(encoder)
                }
            } else {
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #decode_args
                    #call;
                    Ok(#krate::__rt::EncodedData::default())
                }
            }
        }
        ExportMethodKind::StaticMethod => {
            // Static method: just call directly
            if let Some(ret_ty) = &method.ret {
                quote_spanned! {span=>
                    #decode_args
                    let result = #class::#rust_name(#(#arg_names),*);
                    let mut encoder = #krate::__rt::EncodedData::default();
                    <#ret_ty as #krate::__rt::BinaryEncode>::encode(result, &mut encoder);
                    Ok(encoder)
                }
            } else {
                quote_spanned! {span=>
                    #decode_args
                    #class::#rust_name(#(#arg_names),*);
                    Ok(#krate::__rt::EncodedData::default())
                }
            }
        }
        ExportMethodKind::Getter { property: _ } => {
            // Property getter: call the getter method
            if let Some(ret_ty) = &method.ret {
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #krate::__rt::object_store::with_object::<#class, _>(handle, |obj| {
                        let result = obj.#rust_name();
                        let mut encoder = #krate::__rt::EncodedData::default();
                        <#ret_ty as #krate::__rt::BinaryEncode>::encode(result, &mut encoder);
                        Ok(encoder)
                    })
                }
            } else {
                return Err(syn::Error::new(span, "getter must have a return type"));
            }
        }
        ExportMethodKind::Setter { property: _ } => {
            // Property setter: call the setter method
            let arg_ty = method
                .arguments
                .first()
                .map(|a| &a.ty)
                .ok_or_else(|| syn::Error::new(span, "setter must have an argument"))?;
            let arg_name = method.arguments.first().map(|a| &a.name).unwrap();

            quote_spanned! {span=>
                let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                let #arg_name = <#arg_ty as #krate::__rt::BinaryDecode>::decode(decoder)?;
                #krate::__rt::object_store::with_object_mut::<#class, _>(handle, |obj| {
                    obj.#rust_name(#arg_name);
                });
                Ok(#krate::__rt::EncodedData::default())
            }
        }
    };

    // Generate the actual impl method
    let vis = &method.vis;
    let body = &method.body;
    let rust_attrs = method.fn_rust_attrs();

    let fn_args: Vec<_> = arg_names
        .iter()
        .zip(arg_types.iter())
        .map(|(name, ty)| quote_spanned! {span=> #name: #ty })
        .collect();

    let ret_type = match &method.ret {
        Some(ty) => quote_spanned! {span=> -> #ty },
        None => quote_spanned! {span=> },
    };

    let allows = clippy_allows();
    let method_impl = match &method.kind {
        ExportMethodKind::Constructor | ExportMethodKind::StaticMethod => {
            // No self parameter
            quote_spanned! {span=>
                impl #class {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name(#(#fn_args),*) #ret_type #body
                }
            }
        }
        ExportMethodKind::Method { self_ty } => {
            let receiver = match self_ty {
                SelfType::RefShared => quote_spanned! {span=> &self },
                SelfType::RefMutable => quote_spanned! {span=> &mut self },
                SelfType::ByValue => quote_spanned! {span=> self },
            };
            let fn_args_with_self = if fn_args.is_empty() {
                quote_spanned! {span=> #receiver }
            } else {
                quote_spanned! {span=> #receiver, #(#fn_args),* }
            };
            quote_spanned! {span=>
                impl #class {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name(#fn_args_with_self) #ret_type #body
                }
            }
        }
        ExportMethodKind::Getter { .. } => {
            quote_spanned! {span=>
                impl #class {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name(&self) #ret_type #body
                }
            }
        }
        ExportMethodKind::Setter { .. } => {
            quote_spanned! {span=>
                impl #class {
                    #allows
                    #rust_attrs
                    #vis fn #rust_name(&mut self, #(#fn_args),*) #body
                }
            }
        }
    };

    // Generate JsClassMemberSpec for the method
    let arg_count = method.arguments.len();
    let member_arg_types: Vec<TokenStream> = method
        .arguments
        .iter()
        .map(|arg| {
            let ty = &arg.ty;
            quote_spanned! {span=> #ty }
        })
        .collect();
    let member_return_type = match &method.kind {
        ExportMethodKind::Constructor => {
            Some(quote_spanned! {span=> #krate::__rt::object_store::ObjectHandle })
        }
        ExportMethodKind::Setter { .. } => None,
        _ => method.ret.as_ref().map(|ty| quote_spanned! {span=> #ty }),
    };
    let member_type_helpers =
        generate_member_type_helpers(&member_arg_types, member_return_type, krate, span);
    let (member_name, member_kind) = match &method.kind {
        ExportMethodKind::Constructor => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Constructor },
        ),
        ExportMethodKind::Method { .. } => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Method },
        ),
        ExportMethodKind::StaticMethod => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::StaticMethod },
        ),
        ExportMethodKind::Getter { property } => (
            property.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Getter },
        ),
        ExportMethodKind::Setter { property } => (
            property.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Setter },
        ),
    };

    let js_class_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__CLASS_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_str },
            member_name: quote_spanned! {span=> #member_name },
            export_name: quote_spanned! {span=> #export_name },
            arg_count: quote_spanned! {span=> #arg_count },
            type_helpers: member_type_helpers,
            member_kind,
        },
        krate,
        span,
    );
    let export_spec = generate_js_export_spec(
        "__EXPORT_SPEC",
        quote_spanned! {span=> #export_name },
        quote_spanned! {span=>
            #method_body
        },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #method_impl
        #export_spec
        #js_class_member_spec
    })
}
