use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use wasm_bindgen_macro_support::ast::{
    self, Export, MethodKind, MethodSelf, OperationKind, StartKind, Struct, StructField,
};

use super::common::{
    ClassMemberSpec, ClassSpec, generate_js_class_member_spec, generate_js_class_spec,
    generate_js_export_spec, generate_js_free_export_spec, generate_member_type_helpers,
    namespace_tokens,
};

fn path_last_segment(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

/// Peel the invisible-delimiter `Type::Group` that `macro_rules!` wraps a
/// `$x:ty` fragment in (and any explicit parentheses) so the underlying type is
/// matched directly. Without this, a macro-substituted argument type such as
/// `&[i32]` reaches codegen as `Group(&[i32])` and fails the `Type::Reference`
/// checks, so its wire type would be the borrowed type itself rather than the
/// owned `Vec<i32>`.
fn unwrap_group(mut ty: &syn::Type) -> &syn::Type {
    loop {
        match ty {
            syn::Type::Group(group) => ty = &group.elem,
            syn::Type::Paren(paren) => ty = &paren.elem,
            _ => return ty,
        }
    }
}

/// Wrap a call expression in an `unsafe` block so an exported `unsafe fn` can be
/// invoked from the generated wrapper. `allow(unused_unsafe)` keeps the safe
/// case (the overwhelming majority of exports) free of warnings.
fn unsafe_call(call: TokenStream, span: proc_macro2::Span) -> TokenStream {
    quote_spanned! {span=>
        {
            #[allow(unused_unsafe)]
            let __wry_ret = unsafe { #call };
            __wry_ret
        }
    }
}

/// The wire return type a sync export advertises to JS for `ret_ty`, projected
/// through `ReturnWasmAbi`. For a `Result<T, E>` (however it is spelled — the
/// projection sees through type aliases) this resolves to `ThrowingResult<T,
/// JsValue>` so JS throws the `Err`; for any other type it resolves to the type
/// itself.
fn sync_return_wire_type(
    ret_ty: &syn::Type,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=>
        <#ret_ty as #krate::convert::ReturnWasmAbi>::Wire
    }
}

/// The body tail that evaluates `call_expr`, encodes it through `ReturnWasmAbi`,
/// and yields `Ok(encoder)`. The `ReturnWasmAbi` impl for `Result` throws its
/// `Err` in JS; every other return value is encoded directly. Any `write_backs`
/// (one per `&mut [T]` argument) append their mutated buffers after the return
/// value, in argument order, for JS to copy back into the caller's arrays.
fn sync_encode_return_body(
    call_expr: TokenStream,
    ret_ty: &syn::Type,
    write_backs: &[TokenStream],
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=>
        let mut encoder = #krate::__rt::EncodedData::default();
        let __wry_ret = #call_expr;
        <#ret_ty as #krate::convert::ReturnWasmAbi>::return_abi(__wry_ret, &mut encoder);
        #(#write_backs)*
        Ok(encoder)
    }
}

/// The `Promise<…>` wire type an async export advertises, projected through
/// `IntoJsResult`. For a `Result<T, E>` return (seen through aliases) the
/// resolution is `T`'s; for any other type it is that type's.
fn async_promise_ty(
    ret: Option<&syn::Type>,
    krate: &TokenStream,
    js_sys: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let ret_ty = ret.cloned().unwrap_or_else(|| syn::parse_quote!(()));
    quote_spanned! {span=>
        #js_sys::Promise<<#ret_ty as #krate::convert::IntoJsResult>::Resolution>
    }
}

/// The async body that awaits `call_expr` and lowers its output to
/// `Result<JsValue, JsValue>` through `IntoJsResult` (a `Result` `Err` becomes a
/// rejected promise). Dispatch is by type, so it sees through type aliases; the
/// no-return case is the `()` output handled by the same trait.
fn async_result_body(
    call_expr: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=>
        #krate::convert::IntoJsResult::into_js_result(#call_expr.await)
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
        <&#promise_ty as #krate::__rt::BinaryEncode>::encode(&__wry_promise, &mut encoder);
        #krate::__rt::core::mem::forget(__wry_promise);
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
    /// For each `&mut [T]` argument, a statement that appends its (mutated)
    /// owned buffer to the response encoder so JS copies it back into the
    /// caller's array. Emitted after the return value, in declaration order.
    write_backs: Vec<TokenStream>,
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
    let mut write_backs = Vec::new();

    for (i, arg) in arguments.iter().enumerate() {
        // Most exports bind their arguments to a plain identifier, but a
        // pattern like `_` (e.g. `pub fn f(_: &[i32])`) is also valid Rust.
        // Synthesize a positional binding for any non-identifier pattern so the
        // wire bytes are still decoded and forwarded by position, mirroring the
        // generated ABI wrapper wasm-bindgen produces.
        let (arg_ident, arg_js_name) = match arg.pat_type.pat.as_ref() {
            syn::Pat::Ident(arg_name) => {
                (arg_name.ident.clone(), arg_name.ident.unraw().to_string())
            }
            _ => (format_ident!("__wry_arg{i}"), format!("arg{i}")),
        };
        let arg_name = &arg_ident;
        arg_names.push(arg_js_name);
        // Peel any `macro_rules!` `$x:ty` group wrapper so a macro-substituted
        // argument type is matched as the reference/slice it really is.
        let arg_ty = unwrap_group(arg.pat_type.ty.as_ref());

        // A shared `&T` argument is decoded through `RefFromBinaryDecode`:
        // exported structs borrow from the store, `str`/slices choose their owned
        // transport, and JS handles ride the borrow stack.
        let is_borrowable_ref = matches!(arg_ty, syn::Type::Reference(reference)
            if reference.mutability.is_none());

        if is_borrowable_ref {
            let syn::Type::Reference(reference) = arg_ty else {
                unreachable!("is_borrowable_ref implies a shared reference");
            };
            let elem = unwrap_group(&reference.elem);
            let anchor_name = format_ident!("__wry_{}_anchor", arg_name);
            wire_types.push(quote_spanned! {span=> <#elem as #krate::convert::RefFromBinaryDecode>::Wire });
            decode_args.extend(quote_spanned! {span=>
                let #anchor_name = <#elem as #krate::convert::RefFromBinaryDecode>::ref_decode(decoder)?;
            });
            borrow_bindings.extend(quote_spanned! {span=>
                let #arg_name = #krate::__rt::core::ops::Deref::deref(&#anchor_name);
            });
            call_args.push(quote_spanned! {span=> #arg_name });
            continue;
        }

        // A mutable `&mut T` argument is decoded through `BorrowMutArg`: exported
        // structs borrow from the store, `&mut [T]` decodes into a `MutSliceArg<T>`
        // guard that writes back after the return value, and other impls can
        // choose their own wire/anchor shape.
        let is_borrowable_mut_ref = matches!(arg_ty, syn::Type::Reference(reference)
            if reference.mutability.is_some());

        if is_borrowable_mut_ref {
            let syn::Type::Reference(reference) = arg_ty else {
                unreachable!("is_borrowable_mut_ref implies a mutable reference");
            };
            let elem = unwrap_group(&reference.elem);
            let anchor_name = format_ident!("__wry_{}_anchor", arg_name);
            wire_types
                .push(quote_spanned! {span=> <#elem as #krate::convert::BorrowMutArg>::Wire });
            decode_args.extend(quote_spanned! {span=>
                let mut #anchor_name = <#elem as #krate::convert::BorrowMutArg>::borrow_mut_decode(decoder)?;
            });
            borrow_bindings.extend(quote_spanned! {span=>
                let #arg_name = #krate::__rt::core::ops::DerefMut::deref_mut(&mut #anchor_name);
            });
            write_backs.push(quote_spanned! {span=>
                <#elem as #krate::convert::BorrowMutArg>::write_back(#anchor_name, &mut encoder);
            });
            call_args.push(quote_spanned! {span=> #arg_name });
            continue;
        }

        let wire_ty = quote_spanned! {span=> #arg_ty };
        wire_types.push(wire_ty.clone());

        decode_args.extend(quote_spanned! {span=>
            let #arg_name = <#wire_ty as #krate::__rt::BinaryDecode>::decode(decoder)?;
        });

        call_args.push(quote_spanned! {span=> #arg_name });
    }

    Ok(DecodedArgs {
        decode_args,
        borrow_bindings,
        call_args,
        wire_types,
        arg_names,
        write_backs,
    })
}

/// The namespace-qualified class identity (`ns1__ns2__Name`, else `Name`),
/// mirroring `wasm_bindgen_shared::qualified_name` (which produces
/// `Struct::qualified_name`) so a struct and its `impl` agree on the key used
/// for export names, the generated JS class, and a wrapper's `__className`.
fn qualified_class_name(js_namespace: Option<&[String]>, name: &str) -> String {
    match js_namespace {
        Some(ns) if !ns.is_empty() => format!("{}__{}", ns.join("__"), name),
        _ => name.to_string(),
    }
}

pub(super) fn generate_export_struct(s: &Struct, krate: &TokenStream) -> syn::Result<TokenStream> {
    let rust_name = &s.rust_name;
    let js_name = &s.js_name;
    let span = rust_name.span();
    // The class identity is the namespace-qualified JS name (`ns1__ns2__JsName`,
    // else `js_name`), so two Rust structs that share an ident or a `js_name` in
    // different namespaces stay distinct in the registry, export names, and
    // wrapper `__className`. JS install still uses the bare `js_name` + namespace.
    let class_name = s.qualified_name.clone();
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

    // Generate field getters and setters. Field accessors key their export and
    // member-spec names off the JS registry identity (`class_name`), so a
    // `js_name`-renamed struct's fields attach to the renamed class.
    let mut field_impls = TokenStream::new();
    for field in &s.fields {
        field_impls.extend(generate_field_accessor(
            rust_name,
            &class_name,
            field,
            krate,
        )?);
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

    // `#[wasm_bindgen(inspectable)]` `toJSON`/`toString` are emitted as pure JS
    // class methods at registry-build time (see `function_registry.rs`), driven by
    // the `inspectable` flag + public-field list on the `JsClassSpec`.

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

    // Generate EncodeTypeDef - an exported struct passed or returned by value
    // rides the JS heap like a `JsValue`, but advertises the distinct `RustValue`
    // wire tag so JS applies moved-value semantics (zero the wrapper's handle on
    // a by-value pass, throw "Attempt to use a moved value" on later use).
    let encode_type_def_impl = quote_spanned! {span=>
        impl #krate::__rt::EncodeTypeDef for #rust_name {
            fn encode_type_def(type_def: &mut #krate::__rt::TypeDef) {
                type_def.rust_value(#class_name);
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

    // An exported struct is a value type (not `JsGeneric`), so it carries the
    // `IntoWasmAbi`/`FromWasmAbi` markers explicitly. This lets it flow through
    // `ReturnWasmAbi` (the direct, non-throwing path) when returned by an export.
    let wasm_abi_impl = quote_spanned! {span=>
        impl #krate::convert::IntoWasmAbi for #rust_name {}
        impl #krate::convert::FromWasmAbi for #rust_name {}
    };

    // An exported struct returned from an `async fn` resolves a promise to
    // itself, so it advertises its own type as the promise resolution.
    let promising_impl = quote_spanned! {span=>
        impl #krate::sys::Promising for #rust_name {
            type Resolution = #rust_name;
        }
    };

    // Borrowed-decode support so this struct can be a `&T` export or callback
    // argument. The routed handle rides the wire as a plain `u32`, decoded and
    // checked out without consuming the wrapper.
    let ref_from_binary_decode_impl = quote_spanned! {span=>
        impl #krate::convert::RefFromBinaryDecode for #rust_name {
            type Wire = #krate::convert::RefArg<#rust_name>;
            type Anchor = #krate::__rt::object_store::ObjectRefAnchor<#rust_name>;
            fn ref_decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self::Anchor, #krate::__rt::DecodeError> {
                #krate::__rt::object_store::ObjectRefAnchor::checkout_from_decoder(decoder)
            }
        }
        // Direct `&mut Self` export argument: a mutable borrow out of the store
        // that composes with the receiver's borrow, so aliasing the receiver
        // (`x.mutate(x)`) reports "recursive use of an object".
        impl #krate::convert::BorrowMutArg for #rust_name {
            type Wire = #krate::convert::RefMutArg<#rust_name>;
            type Anchor = #krate::__rt::object_store::ObjectRefMutAnchor<#rust_name>;
            fn borrow_mut_decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self::Anchor, #krate::__rt::DecodeError> {
                #krate::__rt::object_store::ObjectRefMutAnchor::checkout_from_decoder(decoder)
            }
        }
    };

    // A `JsValue` wrapping this exact exported class can be unwrapped back into
    // the owned Rust value. A type mismatch (or a non-wrapper value) leaves the
    // stored object untouched and returns the value to the caller, so a failed
    // downcast does not invalidate it.
    let try_from_js_value_impl = quote_spanned! {span=>
        impl #krate::convert::TryFromJsValue for #rust_name {
            fn try_from_js_value_ref(value: &#krate::JsValue) -> #krate::__rt::core::option::Option<Self> {
                let handle = #krate::__rt::extract_rust_handle(value)?;
                if #krate::__rt::object_store::object_is::<#rust_name>(handle) {
                    #krate::__rt::core::option::Option::Some(
                        #krate::__rt::object_store::remove_object::<#rust_name>(handle)
                    )
                } else {
                    #krate::__rt::core::option::Option::None
                }
            }
        }
    };
    // The JS names of public (getter-exposed) fields, used by the generated
    // `inspectable` `toJSON` body. Parent fields exist only for Rust-side upcast
    // projection and are never exposed to JS, so omit them.
    let is_inspectable = s.is_inspectable;
    let public_field_names: Vec<&str> = s
        .fields
        .iter()
        .filter(|f| !f.is_parent)
        .map(|f| f.js_name.as_str())
        .collect();
    let public_fields = quote_spanned! {span=> &[#(#public_field_names),*] };
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
            inspectable: quote_spanned! {span=> #is_inspectable },
            public_fields,
        },
        krate,
        span,
    );

    // For an extended struct, derive `AsRef<Parent<DirectParent>>` so generic
    // Rust code can accept any descendant where it expects a `&Parent<Base>` and
    // walk to the shared parent data (the injected `parent` field).
    let as_ref_parent_impl = if let Some(parent_path) = s.extends.as_ref() {
        quote_spanned! {span=>
            impl #krate::__rt::core::convert::AsRef<#krate::Parent<#parent_path>> for #rust_name {
                fn as_ref(&self) -> &#krate::Parent<#parent_path> {
                    &self.parent
                }
            }
        }
    } else {
        TokenStream::new()
    };

    // When this struct `extends` a parent, emit an upcast export. Given an
    // instance handle of this class (or an ancestor view of it, the dual-path
    // checkout handles both), it publishes a separate handle holding a
    // `Parent<DirectParent>` that shares the descendant's parent cell. The JS
    // constructor / `__wrap` chains these to populate every `__handle_<Ancestor>`
    // slot, so an inherited ancestor method dispatched on a descendant reads the
    // ancestor's shared data.
    let upcast_impl = if s.extends.is_some() {
        let upcast_name = format!("__upcast_{class_name}");
        generate_js_export_spec(
            "__UPCAST_SPEC",
            quote_spanned! {span=> #upcast_name },
            quote_spanned! {span=>
                let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                let parent = #krate::__rt::object_store::with_object::<#rust_name, _>(handle, |obj| {
                    #krate::Parent::share_cell(&obj.parent)
                });
                let ancestor = #krate::Parent::from_cell(parent);
                let ancestor_handle = #krate::__rt::object_store::insert_object(ancestor);
                let mut encoder = #krate::__rt::EncodedData::default();
                <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryEncode>::encode(ancestor_handle, &mut encoder);
                Ok(encoder)
            },
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    Ok(quote_spanned! {span=>
        #class_spec
        #field_impls
        #drop_impl
        #upcast_impl
        #as_ref_parent_impl
        #into_jsvalue_impl
        #encode_type_def_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_result_impl
        #wasm_abi_impl
        #promising_impl
        #ref_from_binary_decode_impl
        #try_from_js_value_impl
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
    let write_backs = &decoded_args.write_backs;
    let sync_decode_args = quote_spanned! {span=> #decode_args #borrow_bindings };

    let ret = function.function.ret.as_ref().map(|ret| &ret.r#type);
    let unit_ty: syn::Type = syn::parse_quote!(());
    // The exported function may be `unsafe` (e.g. one that takes raw pointers).
    // Calling it from the generated wrapper requires an `unsafe` block; the
    // `allow(unused_unsafe)` keeps the common safe case warning-free.
    let call_expr = unsafe_call(quote_spanned! {span=> #rust_name(#(#call_args),*) }, span);
    let (export_body, return_type) = if function.function.r#async {
        let promise_ty = async_promise_ty(ret, krate, js_sys, span);
        let async_result = async_result_body(call_expr, krate, span);
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
        let encode_body = sync_encode_return_body(call_expr, ret_ty, write_backs, krate, span);
        (
            quote_spanned! {span=>
                #sync_decode_args
                #encode_body
            },
            Some(sync_return_wire_type(ret_ty, krate, span)),
        )
    } else {
        let encode_body = sync_encode_return_body(call_expr, &unit_ty, write_backs, krate, span);
        (
            quote_spanned! {span=>
                #sync_decode_args
                #encode_body
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
    let variadic = function.function.variadic;
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
        quote_spanned! {span=> #variadic },
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
        quote_spanned! {span=> false },
        krate,
        span,
    );

    Ok(quote_spanned! {span=>
        #export_spec
        #free_export_spec
    })
}

/// Generate getter and setter for a struct field. `class_id` is the JS-side
/// registry key (the struct's namespace-qualified `js_name`); the export names
/// and member-spec class names key off it so a `js_name`-renamed struct's field
/// accessors attach to the renamed class. `struct_name` stays the Rust type for
/// the object-store checkout.
fn generate_field_accessor(
    struct_name: &syn::Ident,
    class_id: &str,
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

    let getter_name = format!("{class_id}::{js_field_name}_get");
    let setter_name = format!("{class_id}::{js_field_name}_set");

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
    let js_class_name = class_id;
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
            consumes_self: quote_spanned! {span=> false },
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
                consumes_self: quote_spanned! {span=> false },
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

/// Bind `__wry_obj` to the receiver of an instance method, honoring how the
/// method takes `self`: a shared/mutable checkout that stays in the store, or a
/// by-value removal that hands ownership to the method. The call site then uses
/// `__wry_obj.method(..)`, with `DerefMut`/ownership making `&self`, `&mut self`,
/// and `self` receivers all type-check.
fn object_checkout_binding(
    self_ty: MethodSelf,
    class: &Ident,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    match self_ty {
        MethodSelf::RefShared => quote_spanned! {span=>
            let __wry_obj = #krate::__rt::object_store::checkout_object_ref::<#class>(handle);
        },
        MethodSelf::RefMutable => quote_spanned! {span=>
            let mut __wry_obj = #krate::__rt::object_store::checkout_object_mut::<#class>(handle);
        },
        MethodSelf::ByValue => quote_spanned! {span=>
            let __wry_obj = #krate::__rt::object_store::remove_object::<#class>(handle);
        },
    }
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
    // Qualify the class identity by namespace so it matches the struct's
    // `qualified_name` (export names, JS class, `__className` all key off this).
    let class_id = qualified_class_name(method.js_namespace.as_deref(), &js_class_str);
    let export_name = format!("{class_id}::{js_name}");

    let decoded_args = generate_decode_args_parts(&method.function.arguments, krate, span)?;
    let decode_args = &decoded_args.decode_args;
    let borrow_bindings = &decoded_args.borrow_bindings;
    let call_args = &decoded_args.call_args;
    let wire_types = &decoded_args.wire_types;
    let write_backs = &decoded_args.write_backs;
    let sync_decode_args = quote_spanned! {span=> #decode_args #borrow_bindings };
    let ret = method.function.ret.as_ref().map(|ret| &ret.r#type);
    let unit_ty: syn::Type = syn::parse_quote!(());
    let is_async = method.function.r#async;

    // Generate the method call and return encoding based on kind
    let method_body = match &method.method_kind {
        MethodKind::Constructor => {
            // Constructor: create new instance and store in object store
            if is_async {
                let promise_ty = quote_spanned! {span=> #js_sys::Promise<#krate::JsValue> };
                let class_name = class_id.clone();
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
                        #(#write_backs)*
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
                            let __wry_obj = #krate::__rt::object_store::checkout_object_ref::<#class>(handle);
                        }
                    }
                    MethodSelf::RefMutable => {
                        quote_spanned! {span=>
                            let mut __wry_obj = #krate::__rt::object_store::checkout_object_mut::<#class>(handle);
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
                let encode_body = sync_encode_return_body(
                    quote_spanned! {span=> #call },
                    ret_ty,
                    write_backs,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #encode_body
                }
            } else {
                let encode_body = sync_encode_return_body(
                    quote_spanned! {span=> #call },
                    &unit_ty,
                    write_backs,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #encode_body
                }
            }
        }
        MethodKind::Operation(operation) if operation.is_static => {
            // Static method: just call directly
            if is_async {
                let promise_ty = async_promise_ty(ret, krate, js_sys, span);
                let async_result = async_result_body(
                    quote_spanned! {span=> #class::#rust_name(#(#call_args),*) },
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
                    write_backs,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    #sync_decode_args
                    #encode_body
                }
            } else {
                let encode_body = sync_encode_return_body(
                    unsafe_call(
                        quote_spanned! {span=> #class::#rust_name(#(#call_args),*) },
                        span,
                    ),
                    &unit_ty,
                    write_backs,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    #sync_decode_args
                    #encode_body
                }
            }
        }
        MethodKind::Operation(operation) if matches!(operation.kind, OperationKind::Getter(_)) => {
            // Property getter: call the getter method
            if let Some(ret_ty) = ret {
                let self_ty = method.method_self.unwrap_or(MethodSelf::RefShared);
                let object_checkout = object_checkout_binding(self_ty, class, krate, span);
                if is_async {
                    let promise_ty = async_promise_ty(ret, krate, js_sys, span);
                    let async_result = async_result_body(
                        quote_spanned! {span=> __wry_obj.#rust_name() },
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
                        #object_checkout
                        #encode_body
                    }
                } else {
                    // A getter takes no value arguments, so it has no `&mut [T]`
                    // write-backs.
                    let encode_body = sync_encode_return_body(
                        quote_spanned! {span=> __wry_obj.#rust_name() },
                        ret_ty,
                        &[],
                        krate,
                        span,
                    );
                    quote_spanned! {span=>
                        let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                        #object_checkout
                        #encode_body
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

            let self_ty = method.method_self.unwrap_or(MethodSelf::RefMutable);
            let object_checkout = object_checkout_binding(self_ty, class, krate, span);
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
                    #object_checkout
                    #encode_body
                }
            } else {
                let encode_body = sync_encode_return_body(
                    quote_spanned! {span=> __wry_obj.#rust_name(#(#call_args),*) },
                    &unit_ty,
                    write_backs,
                    krate,
                    span,
                );
                quote_spanned! {span=>
                    let handle = <#krate::__rt::object_store::ObjectHandle as #krate::__rt::BinaryDecode>::decode(decoder)?;
                    #sync_decode_args
                    #object_checkout
                    #encode_body
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
            let ret_ty = ret.cloned().unwrap_or_else(|| syn::parse_quote!(()));
            Some(quote_spanned! {span=>
                #js_sys::Promise<<#ret_ty as #krate::convert::IntoJsResult>::Resolution>
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
        // Getters/setters are property accessors regardless of receiver: a
        // static one (no `self`) installs as `static get`/`static set` and is
        // keyed by its property name, not the function's JS name.
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Getter(property),
            is_static,
            ..
        }) => {
            let name = property
                .clone()
                .unwrap_or_else(|| method.function.infer_getter_property().to_string());
            let kind = if *is_static {
                quote! { #krate::__rt::JsClassMemberKind::StaticGetter }
            } else {
                quote! { #krate::__rt::JsClassMemberKind::Getter }
            };
            (name, kind)
        }
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Setter(property),
            is_static,
            ..
        }) => {
            let name = match property {
                Some(property) => property.clone(),
                None => method
                    .function
                    .infer_setter_property()
                    .map_err(|_| syn::Error::new(span, "setter must start with `set_`"))?,
            };
            let kind = if *is_static {
                quote! { #krate::__rt::JsClassMemberKind::StaticSetter }
            } else {
                quote! { #krate::__rt::JsClassMemberKind::Setter }
            };
            (name, kind)
        }
        MethodKind::Operation(operation) if operation.is_static => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::StaticMethod },
        ),
        MethodKind::Operation(_) => (
            js_name.clone(),
            quote! { #krate::__rt::JsClassMemberKind::Method },
        ),
    };

    // A non-static instance member that takes `self` by value consumes the
    // receiver, so the generated JS wrapper zeroes `this.__handle` afterward.
    // Constructors and static members have no receiver to consume.
    let consumes_self = matches!(
        method.method_kind,
        MethodKind::Operation(ast::Operation {
            is_static: false,
            ..
        })
    ) && matches!(method.method_self, Some(MethodSelf::ByValue));
    let js_class_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__CLASS_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_id },
            member_name: quote_spanned! {span=> #member_name },
            export_name: quote_spanned! {span=> #export_name },
            arg_count: quote_spanned! {span=> #arg_count },
            type_helpers: member_type_helpers,
            member_kind,
            consumes_self: quote_spanned! {span=> #consumes_self },
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
