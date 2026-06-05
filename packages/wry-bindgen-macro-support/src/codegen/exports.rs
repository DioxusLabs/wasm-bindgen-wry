use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use wasm_bindgen_macro_support::ast::{
    self, Export, MethodKind, MethodSelf, OperationKind, StartKind, Struct, StructField,
};

use super::common::{
    ClassMemberSpec, ClassSpec, extract_result_ok_type, generate_js_class_member_spec,
    generate_js_class_spec, generate_js_export_spec, generate_js_free_export_spec,
    generate_member_type_helpers, namespace_tokens,
};

fn path_last_segment(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn is_str_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(path) if path.path.is_ident("str"))
}

fn borrowed_arg_wire_type(
    elem: &syn::Type,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    match elem {
        syn::Type::Path(path) if path.path.is_ident("str") => {
            quote_spanned! {span=> #krate::__rt::alloc::string::String }
        }
        syn::Type::Slice(slice) => {
            let elem = &slice.elem;
            quote_spanned! {span=> #krate::__rt::alloc::vec::Vec<#elem> }
        }
        _ => quote_spanned! {span=> #elem },
    }
}

fn arg_wire_type(ty: &syn::Type, krate: &TokenStream, span: proc_macro2::Span) -> TokenStream {
    match ty {
        syn::Type::Reference(reference) => borrowed_arg_wire_type(&reference.elem, krate, span),
        _ => quote_spanned! {span=> #ty },
    }
}

/// The wire return type a sync export advertises to JS for `ret_ty`. A
/// `Result<T, E>` becomes `ThrowingResult<T, JsValue>` so JS throws the `Err`
/// value rather than handing back a `{err}` object, matching wasm-bindgen.
fn sync_return_wire_type(
    ret_ty: &syn::Type,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    match extract_result_ok_type(ret_ty) {
        Some(ok_ty) => quote_spanned! {span=>
            #krate::__rt::ThrowingResult<#ok_ty, #krate::JsValue>
        },
        None => quote_spanned! {span=> #ret_ty },
    }
}

/// The body tail that evaluates `call_expr`, encodes it as `ret_ty`'s wire type,
/// and yields `Ok(encoder)`. A `Result<T, E>` return is encoded as a
/// `ThrowingResult` (its `Err` converted to `JsValue`) so JS throws on error.
fn sync_encode_return_body(
    call_expr: TokenStream,
    ret_ty: &syn::Type,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    if let Some(ok_ty) = extract_result_ok_type(ret_ty) {
        let throwing_ty = quote_spanned! {span=>
            #krate::__rt::ThrowingResult<#ok_ty, #krate::JsValue>
        };
        quote_spanned! {span=>
            let result = #krate::__rt::core::result::Result::map_err(
                #call_expr,
                #krate::__rt::core::convert::Into::<#krate::JsValue>::into,
            );
            let mut encoder = #krate::__rt::EncodedData::default();
            <#throwing_ty as #krate::__rt::BinaryEncode>::encode(
                #krate::__rt::ThrowingResult(result),
                &mut encoder,
            );
            Ok(encoder)
        }
    } else {
        quote_spanned! {span=>
            let result = #call_expr;
            let mut encoder = #krate::__rt::EncodedData::default();
            <#ret_ty as #krate::__rt::BinaryEncode>::encode(result, &mut encoder);
            Ok(encoder)
        }
    }
}

fn async_promise_ty(
    ret: Option<&syn::Type>,
    krate: &TokenStream,
    js_sys: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let ok_ty = ret
        .and_then(extract_result_ok_type)
        .or_else(|| ret.cloned())
        .unwrap_or_else(|| syn::parse_quote!(()));
    quote_spanned! {span=>
        #js_sys::Promise<<#ok_ty as #krate::sys::Promising>::Resolution>
    }
}

fn async_result_body(
    call_expr: TokenStream,
    ret: Option<&syn::Type>,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    if ret.is_none() {
        quote_spanned! {span=>
            #call_expr.await;
            #krate::__rt::core::result::Result::Ok(#krate::JsValue::UNDEFINED)
        }
    } else if ret.and_then(extract_result_ok_type).is_some() {
        quote_spanned! {span=>
            #call_expr
                .await
                .map(#krate::__rt::core::convert::Into::<#krate::JsValue>::into)
                .map_err(#krate::__rt::core::convert::Into::<#krate::JsValue>::into)
        }
    } else {
        quote_spanned! {span=>
            #krate::__rt::core::result::Result::Ok(
                #krate::__rt::core::convert::Into::<#krate::JsValue>::into(
                    #call_expr.await
                )
            )
        }
    }
}

fn encode_async_promise_body(
    promise_ty: TokenStream,
    future_body: TokenStream,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=>
        let __wry_future = async move {
            #future_body
        };
        let __wry_promise = <#js_sys::Promise as #krate::JsCast>::unchecked_into::<#promise_ty>(
            #futures::future_to_promise(__wry_future)
        );
        let mut encoder = #krate::__rt::EncodedData::default();
        <#promise_ty as #krate::__rt::BinaryEncode>::encode(__wry_promise, &mut encoder);
        Ok(encoder)
    }
}

struct DecodedArgs {
    decode_args: TokenStream,
    borrow_bindings: TokenStream,
    call_args: Vec<TokenStream>,
    wire_types: Vec<TokenStream>,
    /// The Rust parameter names, in declaration order, so the generated JS
    /// wrapper exposes them through `Function.prototype.toString` exactly as
    /// wasm-bindgen does.
    arg_names: Vec<String>,
}

fn generate_decode_args_parts(
    arguments: &[ast::FunctionArgumentData],
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> syn::Result<DecodedArgs> {
    let mut decode_args = TokenStream::new();
    let mut borrow_bindings = TokenStream::new();
    let mut call_args = Vec::with_capacity(arguments.len());
    let mut wire_types = Vec::with_capacity(arguments.len());
    let mut arg_names = Vec::with_capacity(arguments.len());

    for arg in arguments {
        let syn::Pat::Ident(arg_name) = arg.pat_type.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &arg.pat_type.pat,
                "complex patterns are not supported by wry-bindgen codegen",
            ));
        };
        let arg_name = &arg_name.ident;
        arg_names.push(arg_name.unraw().to_string());
        let arg_ty = arg.pat_type.ty.as_ref();
        let wire_ty = arg_wire_type(arg_ty, krate, span);
        wire_types.push(wire_ty.clone());

        match arg_ty {
            syn::Type::Reference(reference) => {
                let owned_name = format_ident!("__wry_{}_owned", arg_name);
                let elem = &reference.elem;
                let mutability = reference.mutability;
                let binding = match elem.as_ref() {
                    elem if is_str_type(elem) => {
                        quote_spanned! {span=> let #arg_name = #owned_name.as_str(); }
                    }
                    syn::Type::Slice(_) if mutability.is_some() => {
                        quote_spanned! {span=> let #arg_name = #owned_name.as_mut_slice(); }
                    }
                    syn::Type::Slice(_) => {
                        quote_spanned! {span=> let #arg_name = #owned_name.as_slice(); }
                    }
                    _ if mutability.is_some() => {
                        quote_spanned! {span=> let #arg_name = &mut #owned_name; }
                    }
                    _ => quote_spanned! {span=> let #arg_name = &#owned_name; },
                };
                let let_mut = mutability;
                decode_args.extend(quote_spanned! {span=>
                    let #let_mut #owned_name = <#wire_ty as #krate::__rt::BinaryDecode>::decode(decoder)?;
                });
                borrow_bindings.extend(quote_spanned! {span=>
                    #binding
                });
            }
            _ => {
                decode_args.extend(quote_spanned! {span=>
                    let #arg_name = <#wire_ty as #krate::__rt::BinaryDecode>::decode(decoder)?;
                });
            }
        }

        call_args.push(quote_spanned! {span=> #arg_name });
    }

    Ok(DecodedArgs {
        decode_args,
        borrow_bindings,
        call_args,
        wire_types,
        arg_names,
    })
}

pub(super) fn generate_export_struct(s: &Struct, krate: &TokenStream) -> syn::Result<TokenStream> {
    let rust_name = &s.rust_name;
    let js_name = &s.js_name;
    let span = rust_name.span();
    let class_name = rust_name.to_string();
    let js_namespace = namespace_tokens(s.js_namespace.as_deref(), span);
    let extends = s
        .extends
        .as_ref()
        .and_then(path_last_segment)
        .map(|name| quote_spanned! {span=> #krate::__rt::core::option::Option::Some(#name) })
        .unwrap_or_else(|| quote_spanned! {span=> #krate::__rt::core::option::Option::None });
    let extends_js_class = s
        .extends_js_class
        .as_ref()
        .map(|name| quote_spanned! {span=> #krate::__rt::core::option::Option::Some(#name) })
        .unwrap_or_else(|| quote_spanned! {span=> #krate::__rt::core::option::Option::None });
    let extends_js_namespace = namespace_tokens(s.extends_js_namespace.as_deref(), span);
    let private = s.private;

    // Generate field getters and setters
    let mut field_impls = TokenStream::new();
    for field in &s.fields {
        field_impls.extend(generate_field_accessor(rust_name, field, krate)?);
    }

    // Generate drop function
    let drop_fn_name = format!("{class_name}::__drop");
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
        generate_inspectable(rust_name, &s.fields, &class_name, js_name, krate)?
    } else {
        TokenStream::new()
    };

    // Generate From<StructName> for JsValue - inserts into object store and returns handle
    let into_jsvalue_impl = quote_spanned! {span=>
        impl #krate::__rt::core::convert::From<#rust_name> for #krate::JsValue {
            fn from(val: #rust_name) -> Self {
                let handle = #krate::__rt::object_store::insert_object(val);
                // Create a JS object wrapper with the handle
                #krate::__rt::object_store::create_js_wrapper(handle, #class_name)
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
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self, #krate::__rt::DecodeError> {
                // Decode the JsValue
                let js = #krate::JsValue::decode(decoder)?;
                // Extract handle from JS wrapper
                let handle = #krate::__rt::extract_rust_handle(&js)
                    .ok_or_else(|| #krate::__rt::DecodeError::custom(
                        #krate::__rt::alloc::string::String::from("expected Rust object wrapper")
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
    let class_spec = generate_js_class_spec(
        ClassSpec {
            static_name: "__CLASS_SPEC",
            class_name: quote_spanned! {span=> #class_name },
            js_name: quote_spanned! {span=> #js_name },
            js_namespace,
            private: quote_spanned! {span=> #private },
            extends,
            extends_js_class,
            extends_js_namespace,
        },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #class_spec
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

pub(super) fn generate_export_function(
    function: &Export,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
) -> syn::Result<TokenStream> {
    let rust_name = &function.rust_name;
    let js_name = &function.function.name;
    let span = rust_name.span();

    let decoded_args = generate_decode_args_parts(&function.function.arguments, krate, span)?;
    let decode_args = &decoded_args.decode_args;
    let borrow_bindings = &decoded_args.borrow_bindings;
    let call_args = &decoded_args.call_args;
    let wire_types = &decoded_args.wire_types;
    let sync_decode_args = quote_spanned! {span=> #decode_args #borrow_bindings };

    let ret = function.function.ret.as_ref().map(|ret| &ret.r#type);
    let (export_body, return_type) = if function.function.r#async {
        let promise_ty = async_promise_ty(ret, krate, js_sys, span);
        let async_result = async_result_body(
            quote_spanned! {span=> #rust_name(#(#call_args),*) },
            ret,
            krate,
            span,
        );
        let future_body = quote_spanned! {span=>
            #borrow_bindings
            #async_result
        };
        let encode_body = encode_async_promise_body(
            promise_ty.clone(),
            future_body,
            krate,
            js_sys,
            futures,
            span,
        );
        (
            quote_spanned! {span=>
                #decode_args
                #encode_body
            },
            Some(promise_ty),
        )
    } else if let Some(ret_ty) = ret {
        // A `Result<T, E>` return throws its `Err` in JS via `ThrowingResult`;
        // see `sync_encode_return_body`.
        let encode_body = sync_encode_return_body(
            quote_spanned! {span=> #rust_name(#(#call_args),*) },
            ret_ty,
            krate,
            span,
        );
        (
            quote_spanned! {span=>
                #sync_decode_args
                #encode_body
            },
            Some(sync_return_wire_type(ret_ty, krate, span)),
        )
    } else {
        (
            quote_spanned! {span=>
                #sync_decode_args
                #rust_name(#(#call_args),*);
                Ok(#krate::__rt::EncodedData::default())
            },
            None,
        )
    };

    let export_spec = generate_js_export_spec(
        "__FREE_EXPORT_SPEC",
        quote_spanned! {span=> #js_name },
        export_body,
        krate,
        span,
    );

    let type_helpers = generate_member_type_helpers(wire_types, return_type, krate, span);
    let this = matches!(
        function.method_kind,
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::RegularThis,
            ..
        })
    );
    let arg_count = if this {
        function.function.arguments.len().saturating_sub(1)
    } else {
        function.function.arguments.len()
    };
    // The JS-visible parameter names drop the receiver for `this`-style exports.
    let js_arg_names: Vec<&str> = decoded_args
        .arg_names
        .iter()
        .skip(if this { 1 } else { 0 })
        .map(String::as_str)
        .collect();
    let arg_names = quote_spanned! {span=> &[#(#js_arg_names),*] };
    let namespace = namespace_tokens(function.js_namespace.as_deref(), span);
    let public = !matches!(function.start, StartKind::Private);
    let start = function.start.is_start();
    let free_export_spec = generate_js_free_export_spec(
        "__FREE_EXPORT_JS_SPEC",
        quote_spanned! {span=> #js_name },
        namespace,
        quote_spanned! {span=> #arg_count },
        arg_names,
        type_helpers,
        quote_spanned! {span=> #this },
        quote_spanned! {span=> #public },
        quote_spanned! {span=> #start },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #export_spec
        #free_export_spec
    })
}

pub(super) fn generate_main_function(
    main: &syn::Ident,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let span = main.span();
    let export_name = "__wry_bindgen_main";
    let export_spec = generate_js_export_spec(
        "__MAIN_EXPORT_SPEC",
        quote_spanned! {span=> #export_name },
        quote_spanned! {span=>
            #main();
            Ok(#krate::__rt::EncodedData::default())
        },
        krate,
        span,
    );
    let type_helpers = generate_member_type_helpers(&[], None, krate, span);
    let free_export_spec = generate_js_free_export_spec(
        "__MAIN_FREE_EXPORT_SPEC",
        quote_spanned! {span=> #export_name },
        namespace_tokens(None, span),
        quote_spanned! {span=> 0usize },
        quote_spanned! {span=> &[] },
        type_helpers,
        quote_spanned! {span=> false },
        quote_spanned! {span=> false },
        quote_spanned! {span=> true },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #export_spec
        #free_export_spec
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

    if field.is_parent {
        return Ok(TokenStream::new());
    }

    let struct_name_str = struct_name.to_string();
    let getter_name = format!("{struct_name_str}::{js_field_name}_get");
    let setter_name = format!("{struct_name_str}::{js_field_name}_set");

    // Generate getter
    let getter_body = if field.getter_with_clone.is_some() {
        quote_spanned! {span=>
            #krate::__rt::object_store::with_object::<#struct_name, _>(handle, |obj| {
                let val = #krate::__rt::core::clone::Clone::clone(&obj.#field_name);
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
    class_name: &str,
    js_name: &str,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let span = struct_name.span();
    let to_json_name = format!("{class_name}::toJSON");
    let to_string_name = format!("{class_name}::toString");

    // Build JSON object from fields
    let field_names: Vec<_> = fields
        .iter()
        .filter(|f| !f.is_parent)
        .map(|f| &f.js_name)
        .collect();
    let field_idents: Vec<_> = fields
        .iter()
        .filter(|f| !f.is_parent)
        .map(|f| &f.rust_name)
        .collect();

    let js_name_str = js_name.to_string();
    let class_name_str = class_name.to_string();
    let string_return_type = Some(quote_spanned! {span=> #krate::__rt::alloc::string::String });
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
                let mut json = #krate::__rt::alloc::string::String::from("{");
                #(
                    json.push_str(&#krate::__rt::alloc::format!("\"{}\":{:?},", #field_names, obj.#field_idents));
                )*
                if json.ends_with(',') {
                    json.pop();
                }
                json.push('}');
                let mut encoder = #krate::__rt::EncodedData::default();
                <#krate::__rt::alloc::string::String as #krate::__rt::BinaryEncode>::encode(json, &mut encoder);
                Ok(encoder)
            })
        },
        krate,
        span,
    );
    let to_json_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__TO_JSON_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_name_str },
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
                let s = #krate::__rt::alloc::format!("[object {}]", #js_name_str);
                let mut encoder = #krate::__rt::EncodedData::default();
                <#krate::__rt::alloc::string::String as #krate::__rt::BinaryEncode>::encode(s, &mut encoder);
                Ok(encoder)
            })
        },
        krate,
        span,
    );
    let to_string_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__TO_STRING_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_name_str },
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

/// Generate code for an exported method
pub(super) fn generate_export_method(
    method: &Export,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
) -> syn::Result<TokenStream> {
    let class = method
        .rust_class
        .as_ref()
        .ok_or_else(|| syn::Error::new(method.rust_name.span(), "missing upstream Rust class"))?;
    let rust_name = &method.rust_name;
    let js_name = &method.function.name;
    let span = rust_name.span();

    let class_key = class.unraw().to_string();
    let js_class_str = method.js_class.clone().unwrap_or_else(|| class_key.clone());
    let export_name = format!("{js_class_str}::{js_name}");

    let decoded_args = generate_decode_args_parts(&method.function.arguments, krate, span)?;
    let decode_args = &decoded_args.decode_args;
    let borrow_bindings = &decoded_args.borrow_bindings;
    let call_args = &decoded_args.call_args;
    let wire_types = &decoded_args.wire_types;
    let sync_decode_args = quote_spanned! {span=> #decode_args #borrow_bindings };
    let ret = method.function.ret.as_ref().map(|ret| &ret.r#type);
    let is_async = method.function.r#async;

    // Generate the method call and return encoding based on kind
    let method_body = match &method.method_kind {
        MethodKind::Constructor => {
            // Constructor: create new instance and store in object store
            if is_async {
                let promise_ty = quote_spanned! {span=> #js_sys::Promise<#krate::JsValue> };
                let class_name = class_key.clone();
                let future_body = quote_spanned! {span=>
                    #borrow_bindings
                    let result = #class::#rust_name(#(#call_args),*).await;
                    let handle = #krate::__rt::object_store::insert_object(result);
                    #krate::__rt::core::result::Result::Ok(
                        #krate::__rt::object_store::create_js_wrapper(handle, #class_name)
                    )
                };
                let encode_body = encode_async_promise_body(
                    promise_ty,
                    future_body,
                    krate,
                    js_sys,
                    futures,
                    span,
                );
                quote_spanned! {span=>
                    #decode_args
                    #encode_body
                }
            } else {
                quote_spanned! {span=>
                        #sync_decode_args
                        let result = #class::#rust_name(#(#call_args),*);
                        let handle = #krate::__rt::object_store::insert_object(result);
                        let mut encoder = #krate::__rt::EncodedData::default();
                        <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryEncode>::encode(handle, &mut encoder);
                    Ok(encoder)
                }
            }
        }
        MethodKind::Operation(operation)
            if matches!(
                operation.kind,
                OperationKind::Regular | OperationKind::RegularThis
            ) && !operation.is_static =>
        {
            // Instance method: get object from store, call method
            let self_ty = method
                .method_self
                .ok_or_else(|| syn::Error::new(span, "missing upstream method self"))?;
            let call = match self_ty {
                MethodSelf::RefShared => {
                    quote_spanned! {span=>
                        #krate::__rt::object_store::with_object::<#class, _>(handle, |obj| {
                            obj.#rust_name(#(#call_args),*)
                        })
                    }
                }
                MethodSelf::RefMutable => {
                    quote_spanned! {span=>
                        #krate::__rt::object_store::with_object_mut::<#class, _>(handle, |obj| {
                            obj.#rust_name(#(#call_args),*)
                        })
                    }
                }
                MethodSelf::ByValue => {
                    // Consuming method: remove from store
                    quote_spanned! {span=>
                        {
                            let obj = #krate::__rt::object_store::remove_object::<#class>(handle);
                            obj.#rust_name(#(#call_args),*)
                        }
                    }
                }
            };

            if is_async {
                let promise_ty = async_promise_ty(ret, krate, js_sys, span);
                let object_checkout = match self_ty {
                    MethodSelf::RefShared => {
                        quote_spanned! {span=>
                            let __wry_obj = #krate::__rt::object_store::checkout_object::<#class>(handle);
                        }
                    }
                    MethodSelf::RefMutable => {
                        quote_spanned! {span=>
                            let mut __wry_obj = #krate::__rt::object_store::checkout_object::<#class>(handle);
                        }
                    }
                    MethodSelf::ByValue => {
                        quote_spanned! {span=>
                            let __wry_obj = #krate::__rt::object_store::remove_object::<#class>(handle);
                        }
                    }
                };
                let async_result = async_result_body(
                    quote_spanned! {span=> __wry_obj.#rust_name(#(#call_args),*) },
                    ret,
                    krate,
                    span,
                );
                let future_body = quote_spanned! {span=>
                    #borrow_bindings
                    #async_result
                };
                let encode_body = encode_async_promise_body(
                    promise_ty,
                    future_body,
                    krate,
                    js_sys,
                    futures,
                    span,
                );
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #decode_args
                    #object_checkout
                    #encode_body
                }
            } else if let Some(ret_ty) = ret {
                let encode_body =
                    sync_encode_return_body(quote_spanned! {span=> #call }, ret_ty, krate, span);
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #encode_body
                }
            } else {
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #call;
                    Ok(#krate::__rt::EncodedData::default())
                }
            }
        }
        MethodKind::Operation(operation) if operation.is_static => {
            // Static method: just call directly
            if is_async {
                let promise_ty = async_promise_ty(ret, krate, js_sys, span);
                let async_result = async_result_body(
                    quote_spanned! {span=> #class::#rust_name(#(#call_args),*) },
                    ret,
                    krate,
                    span,
                );
                let future_body = quote_spanned! {span=>
                    #borrow_bindings
                    #async_result
                };
                let encode_body = encode_async_promise_body(
                    promise_ty,
                    future_body,
                    krate,
                    js_sys,
                    futures,
                    span,
                );
                quote_spanned! {span=>
                    #decode_args
                    #encode_body
                }
            } else if let Some(ret_ty) = ret {
                let encode_body = sync_encode_return_body(
                    quote_spanned! {span=> #class::#rust_name(#(#call_args),*) },
                    ret_ty,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    #sync_decode_args
                    #encode_body
                }
            } else {
                quote_spanned! {span=>
                    #sync_decode_args
                    #class::#rust_name(#(#call_args),*);
                    Ok(#krate::__rt::EncodedData::default())
                }
            }
        }
        MethodKind::Operation(operation) if matches!(operation.kind, OperationKind::Getter(_)) => {
            // Property getter: call the getter method
            if let Some(ret_ty) = ret {
                if is_async {
                    let promise_ty = async_promise_ty(ret, krate, js_sys, span);
                    let async_result = async_result_body(
                        quote_spanned! {span=> __wry_obj.#rust_name() },
                        ret,
                        krate,
                        span,
                    );
                    let encode_body = encode_async_promise_body(
                        promise_ty,
                        async_result,
                        krate,
                        js_sys,
                        futures,
                        span,
                    );
                    quote_spanned! {span=>
                        let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                        let __wry_obj = #krate::__rt::object_store::checkout_object::<#class>(handle);
                        #encode_body
                    }
                } else {
                    let encode_body = sync_encode_return_body(
                        quote_spanned! {span=> obj.#rust_name() },
                        ret_ty,
                        krate,
                        span,
                    );
                    quote_spanned! {span=>
                        let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                        #krate::__rt::object_store::with_object::<#class, _>(handle, |obj| {
                            #encode_body
                        })
                    }
                }
            } else {
                return Err(syn::Error::new(span, "getter must have a return type"));
            }
        }
        MethodKind::Operation(operation) if matches!(operation.kind, OperationKind::Setter(_)) => {
            // Property setter: call the setter method
            if method.function.arguments.is_empty() {
                return Err(syn::Error::new(span, "setter must have an argument"));
            }

            if is_async {
                let promise_ty = quote_spanned! {span=>
                    #js_sys::Promise<#krate::sys::Undefined>
                };
                let future_body = quote_spanned! {span=>
                    #borrow_bindings
                    __wry_obj.#rust_name(#(#call_args),*).await;
                    #krate::__rt::core::result::Result::Ok(#krate::JsValue::UNDEFINED)
                };
                let encode_body = encode_async_promise_body(
                    promise_ty,
                    future_body,
                    krate,
                    js_sys,
                    futures,
                    span,
                );
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #decode_args
                    let mut __wry_obj = #krate::__rt::object_store::checkout_object::<#class>(handle);
                    #encode_body
                }
            } else {
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #krate::__rt::object_store::with_object_mut::<#class, _>(handle, |obj| {
                        obj.#rust_name(#(#call_args),*);
                    });
                    Ok(#krate::__rt::EncodedData::default())
                }
            }
        }
        MethodKind::Operation(operation)
            if matches!(
                operation.kind,
                OperationKind::IndexingGetter
                    | OperationKind::IndexingSetter
                    | OperationKind::IndexingDeleter
            ) =>
        {
            return Err(syn::Error::new(
                span,
                "wry-bindgen does not yet support indexing operations on exported methods",
            ));
        }
        MethodKind::Operation(_) => {
            return Err(syn::Error::new(
                span,
                "unsupported upstream exported method operation",
            ));
        }
    };

    // Generate the actual impl method
    // Generate JsClassMemberSpec for the method
    let arg_count = method.function.arguments.len();
    let member_return_type = match &method.method_kind {
        MethodKind::Constructor if is_async => {
            Some(quote_spanned! {span=> #js_sys::Promise<#krate::JsValue> })
        }
        MethodKind::Constructor => {
            Some(quote_spanned! {span=> #krate::__rt::object_store::ObjectHandle })
        }
        _ if is_async => {
            let ok_ty = ret
                .and_then(extract_result_ok_type)
                .or_else(|| ret.cloned())
                .unwrap_or_else(|| syn::parse_quote!(()));
            Some(quote_spanned! {span=>
                #js_sys::Promise<<#ok_ty as #krate::sys::Promising>::Resolution>
            })
        }
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Setter(_),
            ..
        }) => None,
        _ => ret.map(|ty| sync_return_wire_type(ty, krate, span)),
    };
    let member_type_helpers =
        generate_member_type_helpers(wire_types, member_return_type, krate, span);
    let (member_name, member_kind) = match &method.method_kind {
        MethodKind::Constructor => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Constructor },
        ),
        MethodKind::Operation(operation) if operation.is_static => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::StaticMethod },
        ),
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Getter(property),
            ..
        }) => (
            property
                .clone()
                .unwrap_or_else(|| method.function.infer_getter_property().to_string()),
            quote! { #krate::__rt::JsClassMemberKind::Getter },
        ),
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Setter(property),
            ..
        }) => (
            match property {
                Some(property) => property.clone(),
                None => method
                    .function
                    .infer_setter_property()
                    .map_err(|_| syn::Error::new(span, "setter must start with `set_`"))?,
            },
            quote! { #krate::__rt::JsClassMemberKind::Setter },
        ),
        MethodKind::Operation(_) => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Method },
        ),
    };

    let js_class_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__CLASS_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_key },
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
        #export_spec
        #js_class_member_spec
    })
}
