use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use wasm_bindgen_macro_support::ast::{
    self, Export, MethodKind, MethodSelf, OperationKind, StartKind, Struct, StructField,
};

use super::common::{
    ClassMemberSpec, ClassSpec, clippy_allows, generate_js_class_member_spec,
    generate_js_class_spec, generate_js_export_registration, namespace_tokens,
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

/// Drop the explicit lifetime from a top-level reference type so the generated
/// export wrapper — which has none of the function's lifetime parameters in
/// scope — can name it in `<#ty as ArgAbi<S>>`. `&'a [u8]` becomes `&[u8]`, whose
/// borrow lifetime is then inferred from the decoded guard; non-reference types
/// are returned unchanged.
fn strip_ref_lifetime(ty: &syn::Type) -> syn::Type {
    match ty {
        syn::Type::Reference(reference) => {
            let mut reference = reference.clone();
            reference.lifetime = None;
            syn::Type::Reference(reference)
        }
        other => other.clone(),
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

/// The wire return type an export advertises to JS for `ret`, projected through
/// `ReturnAbi<S>` — the return-side analog of `<#ty as ArgAbi<S>>::Wire`. A sync
/// (`CallScoped`) export advertises `<ret as ReturnAbi<CallScoped>>::Wire`
/// (`ThrowingResult<T, JsValue>` for a `Result`, else the type itself); an async
/// (`Anchored`) export advertises `Promise<<ret as ReturnAbi<Anchored>>::Wire>`
/// (the resolution, wrapped in the configured `js_sys::Promise`). A missing `ret`
/// is `()`. Dispatch is by type, so it sees through aliases.
fn return_wire_type(
    ret: Option<&syn::Type>,
    is_async: bool,
    krate: &TokenStream,
    js_sys: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let ret_ty = ret.cloned().unwrap_or_else(|| syn::parse_quote!(()));
    if is_async {
        quote_spanned! {span=>
            #js_sys::Promise<<#ret_ty as #krate::convert::ReturnAbi<#krate::convert::Anchored>>::Wire>
        }
    } else {
        quote_spanned! {span=>
            <#ret_ty as #krate::convert::ReturnAbi<#krate::convert::CallScoped>>::Wire
        }
    }
}

fn async_promise_resolver(
    ret: Option<&syn::Type>,
    krate: &TokenStream,
    js_sys: &TokenStream,
    futures: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let promise_ty = return_wire_type(ret, true, krate, js_sys, span);
    quote_spanned! {span=>
        |__wry_future| -> #promise_ty {
            <#js_sys::Promise as #krate::JsCast>::unchecked_into::<#promise_ty>(
                #futures::future_to_promise(__wry_future)
            )
        }
    }
}

fn with_receiver_handle_arg(
    decoded: &DecodedArgs,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> DecodedArgs {
    let mut arg_tys = Vec::with_capacity(decoded.arg_tys.len() + 1);
    let mut arg_idents = Vec::with_capacity(decoded.arg_idents.len() + 1);

    arg_tys.push(syn::parse_quote_spanned! {span=>
        #krate::__rt::object_store::ObjectHandle
    });
    arg_tys.extend(decoded.arg_tys.iter().cloned());
    arg_idents.push(format_ident!("handle"));
    arg_idents.extend(decoded.arg_idents.iter().cloned());

    DecodedArgs {
        arg_tys,
        arg_idents,
        arg_names: decoded.arg_names.clone(),
        scope: decoded.scope.clone(),
    }
}

fn async_receiver_callable_with_handle(
    self_ty: MethodSelf,
    class: &Ident,
    rust_name: &Ident,
    decoded: &DecodedArgs,
    call_args: &[Ident],
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let store = quote_spanned! {span=> #krate::__rt::object_store };
    let params = annotated_params(decoded, krate, span);
    let handle = &decoded.arg_idents[0];
    match self_ty {
        MethodSelf::RefShared => quote_spanned! {span=>
            async move |#params| {
                let __wry_obj = #store::checkout_object_ref::<#class>(#handle);
                __wry_obj.#rust_name(#(#call_args),*).await
            }
        },
        MethodSelf::RefMutable => quote_spanned! {span=>
            async move |#params| {
                let mut __wry_obj = #store::checkout_object_mut::<#class>(#handle);
                __wry_obj.#rust_name(#(#call_args),*).await
            }
        },
        MethodSelf::ByValue => quote_spanned! {span=>
            async move |#params| {
                let __wry_obj = #store::remove_object::<#class>(#handle);
                __wry_obj.#rust_name(#(#call_args),*).await
            }
        },
    }
}

fn receiver_callable_with_handle(
    self_ty: MethodSelf,
    class: &Ident,
    rust_name: &Ident,
    decoded: &DecodedArgs,
    call_args: &[Ident],
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let store = quote_spanned! {span=> #krate::__rt::object_store };
    let params = annotated_params(decoded, krate, span);
    let handle = &decoded.arg_idents[0];
    match self_ty {
        MethodSelf::RefShared => quote_spanned! {span=>
            move |#params| {
                let __wry_obj = #store::checkout_object_ref::<#class>(#handle);
                __wry_obj.#rust_name(#(#call_args),*)
            }
        },
        MethodSelf::RefMutable => quote_spanned! {span=>
            move |#params| {
                let mut __wry_obj = #store::checkout_object_mut::<#class>(#handle);
                __wry_obj.#rust_name(#(#call_args),*)
            }
        },
        MethodSelf::ByValue => quote_spanned! {span=>
            move |#params| {
                let obj = #store::remove_object::<#class>(#handle);
                obj.#rust_name(#(#call_args),*)
            }
        },
    }
}

/// A receiver/wrapper closure's parameter list with each argument explicitly
/// typed as `<Ty as ArgAbi<S>>::Projected<'_>`. The elided lifetime is
/// late-bound, which forces the closure to be higher-ranked (`for<'a>`) — without
/// the annotation a closure taking a *borrowed* projected argument infers one
/// fixed lifetime and fails the `CallExport` bound ("implementation of `Fn`
/// is not general enough").
fn annotated_params(
    decoded: &DecodedArgs,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let scope = &decoded.scope;
    let params = decoded
        .arg_idents
        .iter()
        .zip(&decoded.arg_tys)
        .map(|(ident, ty)| {
            quote_spanned! {span=> #ident: <#ty as #krate::convert::ArgAbi<#scope>>::Projected<'_> }
        });
    quote_spanned! {span=> #(#params),* }
}

/// The callable handed to [`CallExport`]/[`CallExportAsync`] for a free function
/// or static method named by `path`. A safe `fn`/`async fn` satisfies the
/// `Fn`/`AsyncFn` bound directly, so it is passed by path. An `unsafe fn`
/// does not implement those traits, so it is wrapped in a safe closure whose body
/// supplies the `unsafe` block (`allow(unused_unsafe)` keeps the safe case quiet).
fn free_callable(
    path: TokenStream,
    decoded: &DecodedArgs,
    is_unsafe: bool,
    is_async: bool,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    if !is_unsafe {
        return path;
    }
    let params = annotated_params(decoded, krate, span);
    let arg_idents = &decoded.arg_idents;
    let call = unsafe_call(quote_spanned! {span=> #path(#(#arg_idents),*) }, span);
    if is_async {
        quote_spanned! {span=> async move |#params| #call.await }
    } else {
        quote_spanned! {span=> move |#params| #call }
    }
}

#[derive(Clone)]
struct DecodedArgs {
    /// The full spelled argument types, in declaration order. They form the
    /// `(A0, A1, …)` tuple naming the `CallExport`/`CallExportAsync` arity.
    arg_tys: Vec<syn::Type>,
    /// The argument bindings, in declaration order — used as the receiver
    /// closure's parameters and as the call's arguments.
    arg_idents: Vec<Ident>,
    /// The Rust parameter names, in declaration order, so the generated JS
    /// wrapper exposes them through `Function.prototype.toString` exactly as
    /// wasm-bindgen does.
    arg_names: Vec<String>,
    /// The `ArgAbi` borrow scope: `CallScoped` for a sync export, `Anchored`
    /// for an `async` one (whose borrows must outlive the returned `Promise`).
    scope: TokenStream,
}

fn call_export_arg_types(
    decoded: &DecodedArgs,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let arg_tys = &decoded.arg_tys;
    let scope = &decoded.scope;
    quote_spanned! {span=>
        <(#(#arg_tys,)*) as #krate::convert::CallExportArgs<#scope>>::arg_types
    }
}

fn call_export_no_return_type(krate: &TokenStream, span: proc_macro2::Span) -> TokenStream {
    quote_spanned! {span=> || #krate::__rt::TypeDef::of::<()>() }
}

fn call_export_return_type(
    return_type: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=> || #krate::__rt::TypeDef::of::<#return_type>() }
}

fn call_export_type_fns(
    decoded: &DecodedArgs,
    return_type: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> (TokenStream, TokenStream) {
    let arg_types = call_export_arg_types(decoded, krate, span);
    let return_type = call_export_return_type(return_type, krate, span);
    (arg_types, return_type)
}

#[allow(clippy::too_many_arguments)]
fn call_export_free_spec(
    decoded: &DecodedArgs,
    callable: TokenStream,
    is_async: bool,
    resolve_async: Option<TokenStream>,
    export_name: TokenStream,
    namespace: TokenStream,
    arg_names: TokenStream,
    this: TokenStream,
    public: TokenStream,
    start: TokenStream,
    variadic: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let arg_tys = &decoded.arg_tys;
    let scope = &decoded.scope;
    if is_async {
        let resolve_async = resolve_async.expect("async exports always provide a Promise resolver");
        quote_spanned! {span=>
            #krate::convert::CallExportAsync::<(#(#arg_tys,)*), #scope>::export_spec(
                #callable,
                #resolve_async,
                #export_name,
                #namespace,
                #arg_names,
                #this,
                #public,
                #start,
                #variadic,
            )
        }
    } else {
        quote_spanned! {span=>
            #krate::convert::CallExport::<(#(#arg_tys,)*), #scope>::export_spec(
                #callable,
                #export_name,
                #namespace,
                #arg_names,
                #this,
                #public,
                #start,
                #variadic,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn call_export_async_method_spec(
    decoded: &DecodedArgs,
    callable: TokenStream,
    resolve_async: TokenStream,
    export_name: TokenStream,
    arg_names: TokenStream,
    variadic: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let arg_tys = &decoded.arg_tys;
    let scope = &decoded.scope;
    quote_spanned! {span=>
        #krate::convert::CallExportAsync::<(#(#arg_tys,)*), #scope>::export_spec(
            #callable,
            #resolve_async,
            #export_name,
            &[],
            #arg_names,
            false,
            false,
            false,
            #variadic,
        )
    }
}

fn call_export_sync_private_spec(
    decoded: &DecodedArgs,
    callable: TokenStream,
    export_name: TokenStream,
    arg_names: TokenStream,
    variadic: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    call_export_free_spec(
        decoded,
        callable,
        false,
        None,
        export_name,
        quote_spanned! {span=> &[] },
        arg_names,
        quote_spanned! {span=> false },
        quote_spanned! {span=> false },
        quote_spanned! {span=> false },
        variadic,
        krate,
        span,
    )
}

fn call_export_sync_private_registration(
    static_name: &str,
    decoded: &DecodedArgs,
    callable: TokenStream,
    export_name: TokenStream,
    arg_names: TokenStream,
    variadic: TokenStream,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    let export_spec = call_export_sync_private_spec(
        decoded,
        callable,
        export_name,
        arg_names,
        variadic,
        krate,
        span,
    );
    generate_js_export_registration(static_name, export_spec, krate, span)
}

fn call_scoped_arg_types(
    arg_types: &[TokenStream],
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> TokenStream {
    quote_spanned! {span=>
        <(#(#arg_types,)*) as #krate::convert::CallExportArgs<#krate::convert::CallScoped>>::arg_types
    }
}

fn generate_decode_args_parts(
    arguments: &[ast::FunctionArgumentData],
    is_async: bool,
    krate: &TokenStream,
    span: proc_macro2::Span,
) -> syn::Result<DecodedArgs> {
    // The borrow scope selects the `async` variant (`Anchored`), whose borrowed
    // arguments anchor an owned copy that outlives the returned `Promise`; a sync
    // export is `CallScoped`. It is the same for every argument.
    let scope = if is_async {
        quote_spanned! {span=> #krate::convert::Anchored }
    } else {
        quote_spanned! {span=> #krate::convert::CallScoped }
    };
    let mut arg_tys = Vec::with_capacity(arguments.len());
    let mut arg_idents = Vec::with_capacity(arguments.len());
    let mut arg_names = Vec::with_capacity(arguments.len());

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
        arg_names.push(arg_js_name);
        // Peel any `macro_rules!` `$x:ty` group wrapper so a macro-substituted
        // argument type is matched as the reference/slice it really is.
        let arg_ty = unwrap_group(arg.pat_type.ty.as_ref());

        // Key on the *full spelled* type (trait resolution sees through aliases,
        // so `fn f(x: U8Slice)` with `type U8Slice<'a> = &'a [u8]` behaves like
        // `&[u8]`). A reference's explicit lifetime is dropped (`&'a [u8]` ->
        // `&[u8]`) because the generated wrapper has none of the function's
        // lifetime parameters in scope; the borrow lifetime is inferred.
        let arg_ty = strip_ref_lifetime(arg_ty);

        arg_idents.push(arg_ident);
        arg_tys.push(arg_ty);
    }

    Ok(DecodedArgs {
        arg_tys,
        arg_idents,
        arg_names,
        scope,
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
    let drop_handle = format_ident!("handle");
    let drop_args = DecodedArgs {
        arg_tys: vec![syn::parse_quote_spanned! {span=>
            #krate::__rt::object_store::ObjectHandle
        }],
        arg_idents: vec![drop_handle.clone()],
        arg_names: Vec::new(),
        scope: quote_spanned! {span=> #krate::convert::CallScoped },
    };
    let drop_params = annotated_params(&drop_args, krate, span);
    let drop_impl = call_export_sync_private_registration(
        "__DROP_SPEC",
        &drop_args,
        quote_spanned! {span=>
            move |#drop_params| {
                #krate::__rt::object_store::drop_object(#drop_handle);
            }
        },
        quote_spanned! {span=> #drop_fn_name },
        quote_spanned! {span=> &[] },
        quote_spanned! {span=> false },
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

    // Borrowed-argument support so this struct can be a `&T` export or callback
    // argument. The routed handle rides the wire as a plain `u32`, decoded and
    // checked out without consuming the wrapper.
    let allows = clippy_allows();
    let borrow_arg_impls = quote_spanned! {span=>
        // `ArgAbi<S>` for the *full* `&Self`/`&mut Self` argument types, so an
        // exported function decodes a borrowed struct argument through the uniform
        // `<#arg_ty as ArgAbi<S>>` projection — including when the type reaches the
        // macro behind an alias. Callback decoding uses the same shared-borrow impl
        // for borrowed first arguments. A store checkout is valid across an await,
        // so one impl serves both borrow scopes:
        // the synchronous `project` lends the checkout into `with`, while
        // `project_async` moves the checkout into the `async` export's future and
        // lends it across the `.await`.
        #allows
        impl<__WryScope: #krate::convert::BorrowScope> #krate::convert::ArgAbi<__WryScope> for &#rust_name {
            type Wire = #krate::convert::RefArg<#rust_name>;
            type Guard = #krate::__rt::object_store::ObjectRefAnchor<#rust_name>;
            type ProjectedGuard = Self::Guard;
            type Projected<'__wry> = &'__wry #rust_name;
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self::Guard, #krate::__rt::DecodeError> {
                #krate::__rt::object_store::ObjectRefAnchor::checkout_from_decoder(decoder)
            }
            fn project<__WryR, __WryF>(guard: Self::Guard, with: __WryF) -> (__WryR, Self::ProjectedGuard)
            where
                __WryF: for<'__wry> FnOnce(Self::Projected<'__wry>) -> __WryR,
            {
                let __wry_result = with(&*guard);
                (__wry_result, guard)
            }
            fn project_async<__WryR, __WryF>(guard: Self::Guard, with: __WryF) -> impl #krate::__rt::core::future::Future<Output = __WryR>
            where
                __WryF: for<'__wry> #krate::__rt::core::ops::AsyncFnOnce(Self::Projected<'__wry>) -> __WryR,
            {
                async move { with(&*guard).await }
            }
        }
        #allows
        impl<__WryScope: #krate::convert::BorrowScope> #krate::convert::ArgAbi<__WryScope> for &mut #rust_name {
            type Wire = #krate::convert::RefMutArg<#rust_name>;
            type Guard = #krate::__rt::object_store::ObjectRefMutAnchor<#rust_name>;
            type ProjectedGuard = Self::Guard;
            type Projected<'__wry> = &'__wry mut #rust_name;
            fn decode(decoder: &mut #krate::__rt::DecodedData) -> #krate::__rt::core::result::Result<Self::Guard, #krate::__rt::DecodeError> {
                #krate::__rt::object_store::ObjectRefMutAnchor::checkout_from_decoder(decoder)
            }
            fn project<__WryR, __WryF>(mut guard: Self::Guard, with: __WryF) -> (__WryR, Self::ProjectedGuard)
            where
                __WryF: for<'__wry> FnOnce(Self::Projected<'__wry>) -> __WryR,
            {
                let __wry_result = with(&mut *guard);
                (__wry_result, guard)
            }
            fn project_async<__WryR, __WryF>(mut guard: Self::Guard, with: __WryF) -> impl #krate::__rt::core::future::Future<Output = __WryR>
            where
                __WryF: for<'__wry> #krate::__rt::core::ops::AsyncFnOnce(Self::Projected<'__wry>) -> __WryR,
            {
                async move { with(&mut *guard).await }
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
        let upcast_handle = format_ident!("handle");
        let upcast_args = DecodedArgs {
            arg_tys: vec![syn::parse_quote_spanned! {span=>
                #krate::__rt::object_store::ObjectHandle
            }],
            arg_idents: vec![upcast_handle.clone()],
            arg_names: Vec::new(),
            scope: quote_spanned! {span=> #krate::convert::CallScoped },
        };
        let upcast_params = annotated_params(&upcast_args, krate, span);
        call_export_sync_private_registration(
            "__UPCAST_SPEC",
            &upcast_args,
            quote_spanned! {span=>
                move |#upcast_params| {
                    let __wry_obj = #krate::__rt::object_store::checkout_object_ref::<#rust_name>(#upcast_handle);
                    let parent = #krate::Parent::share_cell(&__wry_obj.parent);
                    let ancestor = #krate::Parent::from_cell(parent);
                    #krate::__rt::object_store::insert_object(ancestor)
                }
            },
            quote_spanned! {span=> #upcast_name },
            quote_spanned! {span=> &[] },
            quote_spanned! {span=> false },
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
        #borrow_arg_impls
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

    let decoded_args = generate_decode_args_parts(
        &function.function.arguments,
        function.function.r#async,
        krate,
        span,
    )?;

    let ret = function.function.ret.as_ref().map(|ret| &ret.r#type);
    let is_async = function.function.r#async;
    // The exported function is dispatched through `CallExport`, which decodes,
    // projects, calls, and encodes by arity. A safe `fn` is passed by path; an
    // `unsafe fn` is wrapped in a closure that supplies the `unsafe` block.
    let this = matches!(
        function.method_kind,
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::RegularThis,
            ..
        })
    );
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
    let export_callable = free_callable(
        quote_spanned! {span=> #rust_name },
        &decoded_args,
        function.function.r#unsafe,
        is_async,
        krate,
        span,
    );
    let resolve_async = if is_async {
        Some(async_promise_resolver(ret, krate, js_sys, futures, span))
    } else {
        None
    };
    let free_export = call_export_free_spec(
        &decoded_args,
        export_callable,
        is_async,
        resolve_async,
        quote_spanned! {span=> #js_name },
        namespace,
        arg_names,
        quote_spanned! {span=> #this },
        quote_spanned! {span=> #public },
        quote_spanned! {span=> #start },
        quote_spanned! {span=> #variadic },
        krate,
        span,
    );
    let export_spec =
        generate_js_export_registration("__FREE_EXPORT_SPEC", free_export, krate, span);

    Ok(quote_spanned! {span=>
        #export_spec
    })
}

pub(super) fn generate_main_function(
    main: &syn::Ident,
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    let span = main.span();
    let export_name = "__wry_bindgen_main";
    let decoded_args = DecodedArgs {
        arg_tys: Vec::new(),
        arg_idents: Vec::new(),
        arg_names: Vec::new(),
        scope: quote_spanned! {span=> #krate::convert::CallScoped },
    };
    let free_export = call_export_free_spec(
        &decoded_args,
        quote_spanned! {span=> || { #main(); } },
        false,
        None,
        quote_spanned! {span=> #export_name },
        namespace_tokens(None, span),
        quote_spanned! {span=> &[] },
        quote_spanned! {span=> false },
        quote_spanned! {span=> false },
        quote_spanned! {span=> true },
        quote_spanned! {span=> false },
        krate,
        span,
    );
    let export_spec =
        generate_js_export_registration("__MAIN_EXPORT_SPEC", free_export, krate, span);

    Ok(quote_spanned! {span=>
        #export_spec
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

    let getter_handle = format_ident!("handle");
    let getter_args = DecodedArgs {
        arg_tys: vec![syn::parse_quote_spanned! {span=>
            #krate::__rt::object_store::ObjectHandle
        }],
        arg_idents: vec![getter_handle.clone()],
        arg_names: Vec::new(),
        scope: quote_spanned! {span=> #krate::convert::CallScoped },
    };
    let getter_params = annotated_params(&getter_args, krate, span);

    // Generate getter
    let getter_value = if field.getter_with_clone.is_some() {
        quote_spanned! {span=>
            let __wry_obj = #krate::__rt::object_store::checkout_object_ref::<#struct_name>(#getter_handle);
            #krate::__rt::core::clone::Clone::clone(&__wry_obj.#field_name)
        }
    } else {
        quote_spanned! {span=>
            let __wry_obj = #krate::__rt::object_store::checkout_object_ref::<#struct_name>(#getter_handle);
            __wry_obj.#field_name
        }
    };

    let getter_impl = call_export_sync_private_registration(
        "__GETTER_SPEC",
        &getter_args,
        quote_spanned! {span=>
            move |#getter_params| {
                #getter_value
            }
        },
        quote_spanned! {span=> #getter_name },
        quote_spanned! {span=> &[] },
        quote_spanned! {span=> false },
        krate,
        span,
    );

    // Generate setter (unless readonly)
    let setter_impl = if !field.readonly {
        let setter_handle = format_ident!("handle");
        let setter_val = format_ident!("val");
        let setter_args = DecodedArgs {
            arg_tys: vec![
                syn::parse_quote_spanned! {span=>
                    #krate::__rt::object_store::ObjectHandle
                },
                field.ty.clone(),
            ],
            arg_idents: vec![setter_handle.clone(), setter_val.clone()],
            arg_names: Vec::new(),
            scope: quote_spanned! {span=> #krate::convert::CallScoped },
        };
        let setter_params = annotated_params(&setter_args, krate, span);
        call_export_sync_private_registration(
            "__SETTER_SPEC",
            &setter_args,
            quote_spanned! {span=>
                move |#setter_params| {
                    let mut __wry_obj = #krate::__rt::object_store::checkout_object_mut::<#struct_name>(#setter_handle);
                    __wry_obj.#field_name = #setter_val;
                }
            },
            quote_spanned! {span=> #setter_name },
            quote_spanned! {span=> &[] },
            quote_spanned! {span=> false },
            krate,
            span,
        )
    } else {
        TokenStream::new()
    };

    // Generate JsClassMemberSpec for the property getter
    let js_class_name = class_id;
    let getter_arg_types = call_scoped_arg_types(&[], krate, span);
    let getter_return_type =
        call_export_return_type(quote_spanned! {span=> #field_ty }, krate, span);
    let getter_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__GETTER_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #js_class_name },
            member_name: quote_spanned! {span=> #js_field_name },
            export_name: quote_spanned! {span=> #getter_name },
            arg_types: getter_arg_types,
            return_type: getter_return_type,
            member_kind: quote_spanned! {span=> #krate::__rt::JsClassMemberKind::Getter },
            consumes_self: quote_spanned! {span=> false },
        },
        krate,
        span,
    );

    // Generate JsClassMemberSpec for the property setter (unless readonly)
    let setter_member_spec = if !field.readonly {
        let setter_arg_types = vec![quote_spanned! {span=> #field_ty }];
        let setter_arg_types = call_scoped_arg_types(&setter_arg_types, krate, span);
        let setter_return_type = call_export_no_return_type(krate, span);
        generate_js_class_member_spec(
            ClassMemberSpec {
                static_name: "__SETTER_MEMBER_SPEC",
                class_name: quote_spanned! {span=> #js_class_name },
                member_name: quote_spanned! {span=> #js_field_name },
                export_name: quote_spanned! {span=> #setter_name },
                arg_types: setter_arg_types,
                return_type: setter_return_type,
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

    let decoded_args = generate_decode_args_parts(
        &method.function.arguments,
        method.function.r#async,
        krate,
        span,
    )?;
    let ret = method.function.ret.as_ref().map(|ret| &ret.r#type);
    let is_async = method.function.r#async;

    // Generate the actual impl method
    // Generate JsClassMemberSpec for the method
    let member_return_type = match &method.method_kind {
        MethodKind::Constructor if is_async => {
            quote_spanned! {span=> #js_sys::Promise<#krate::JsValue> }
        }
        MethodKind::Constructor => {
            quote_spanned! {span=> #krate::__rt::object_store::ObjectHandle }
        }
        _ if is_async => return_wire_type(ret, true, krate, js_sys, span),
        MethodKind::Operation(ast::Operation {
            kind: OperationKind::Setter(_),
            ..
        }) => quote_spanned! {span=> () },
        _ => ret
            .map(|ty| return_wire_type(Some(ty), false, krate, js_sys, span))
            .unwrap_or_else(|| quote_spanned! {span=> () }),
    };
    let (member_arg_types, member_return_type) =
        call_export_type_fns(&decoded_args, member_return_type, krate, span);
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
    let variadic = method.function.variadic;
    let js_class_member_spec = generate_js_class_member_spec(
        ClassMemberSpec {
            static_name: "__CLASS_MEMBER_SPEC",
            class_name: quote_spanned! {span=> #class_id },
            member_name: quote_spanned! {span=> #member_name },
            export_name: quote_spanned! {span=> #export_name },
            arg_types: member_arg_types,
            return_type: member_return_type,
            member_kind,
            consumes_self: quote_spanned! {span=> #consumes_self },
        },
        krate,
        span,
    );
    let js_arg_names: Vec<&str> = decoded_args.arg_names.iter().map(String::as_str).collect();
    let arg_names = quote_spanned! {span=> &[#(#js_arg_names),*] };
    let arg_idents = &decoded_args.arg_idents;
    let (export_decoded_args, export_callable) = match &method.method_kind {
        MethodKind::Constructor => {
            let params = annotated_params(&decoded_args, krate, span);
            let callable = if is_async {
                let class_name = class_id.clone();
                quote_spanned! {span=>
                    async move |#params| {
                        let result = #class::#rust_name(#(#arg_idents),*).await;
                        let handle = #krate::__rt::object_store::insert_object(result);
                        #krate::__rt::object_store::create_js_wrapper(handle, #class_name)
                    }
                }
            } else {
                quote_spanned! {span=>
                    move |#params| {
                        #krate::__rt::object_store::insert_object(#class::#rust_name(#(#arg_idents),*))
                    }
                }
            };
            (decoded_args.clone(), callable)
        }
        MethodKind::Operation(operation) if operation.is_static => {
            let callable = free_callable(
                quote_spanned! {span=> #class::#rust_name },
                &decoded_args,
                method.function.r#unsafe,
                is_async,
                krate,
                span,
            );
            (decoded_args.clone(), callable)
        }
        MethodKind::Operation(operation)
            if matches!(
                operation.kind,
                OperationKind::Regular | OperationKind::RegularThis
            ) || matches!(
                operation.kind,
                OperationKind::Getter(_) | OperationKind::Setter(_)
            ) =>
        {
            if matches!(operation.kind, OperationKind::Getter(_)) && ret.is_none() {
                return Err(syn::Error::new(span, "getter must have a return type"));
            }
            if matches!(operation.kind, OperationKind::Setter(_))
                && method.function.arguments.is_empty()
            {
                return Err(syn::Error::new(span, "setter must have an argument"));
            }
            let self_ty = match &operation.kind {
                OperationKind::Getter(_) => method.method_self.unwrap_or(MethodSelf::RefShared),
                OperationKind::Setter(_) => method.method_self.unwrap_or(MethodSelf::RefMutable),
                _ => method
                    .method_self
                    .ok_or_else(|| syn::Error::new(span, "missing upstream method self"))?,
            };
            let decoded_with_handle = with_receiver_handle_arg(&decoded_args, krate, span);
            let callable = if is_async {
                async_receiver_callable_with_handle(
                    self_ty,
                    class,
                    rust_name,
                    &decoded_with_handle,
                    &decoded_args.arg_idents,
                    krate,
                    span,
                )
            } else {
                receiver_callable_with_handle(
                    self_ty,
                    class,
                    rust_name,
                    &decoded_with_handle,
                    &decoded_args.arg_idents,
                    krate,
                    span,
                )
            };
            (decoded_with_handle, callable)
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
    let export_spec = if is_async {
        let resolve_async = match &method.method_kind {
            MethodKind::Constructor => {
                let promise_ty = quote_spanned! {span=> #js_sys::Promise<#krate::JsValue> };
                quote_spanned! {span=>
                    |__wry_future| -> #promise_ty {
                        <#js_sys::Promise as #krate::JsCast>::unchecked_into::<#promise_ty>(
                            #futures::future_to_promise(__wry_future)
                        )
                    }
                }
            }
            MethodKind::Operation(_) => async_promise_resolver(ret, krate, js_sys, futures, span),
        };
        let export_spec = call_export_async_method_spec(
            &export_decoded_args,
            export_callable,
            resolve_async,
            quote_spanned! {span=> #export_name },
            arg_names,
            quote_spanned! {span=> #variadic },
            krate,
            span,
        );
        generate_js_export_registration("__EXPORT_SPEC", export_spec, krate, span)
    } else {
        let export_spec = call_export_sync_private_spec(
            &export_decoded_args,
            export_callable,
            quote_spanned! {span=> #export_name },
            arg_names,
            quote_spanned! {span=> #variadic },
            krate,
            span,
        );
        generate_js_export_registration("__EXPORT_SPEC", export_spec, krate, span)
    };

    Ok(quote_spanned! {span=>
        #export_spec
        #js_class_member_spec
    })
}
