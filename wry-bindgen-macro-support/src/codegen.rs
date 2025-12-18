//! Code generation for wasm_bindgen macro
//!
//! This module generates Rust code that uses the wry-bindgen runtime
//! and inventory-based function registration.

use std::hash::{BuildHasher, Hash, Hasher, RandomState};

use crate::ast::{
    ImportFunction, ImportFunctionKind, ImportStatic, ImportType, Program, StringEnum,
};
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote, quote_spanned};

/// Generate code for the entire program
pub fn generate(program: &Program) -> syn::Result<TokenStream> {
    let mut tokens = TokenStream::new();
    let krate = &program.attrs.crate_path_tokens();

    // First generate the module for inline_js if needed
    let mut prefix = String::new();
    if let Some((span, inline_js_module)) = &program.attrs.inline_js {
        let unique_hash = {
            let s = RandomState::new();
            let mut hasher = s.build_hasher();
            inline_js_module
                .to_token_stream()
                .to_string()
                .hash(&mut hasher);
            hasher.finish()
        };
        let unique_ident = format_ident!("__WRY_BINDGEN_INLINE_JS_MODULE_HASH_{}", unique_hash);
        // Create a static and submit it to the inventory
        tokens.extend(quote_spanned! {*span=>
            static #unique_ident: u64 = {
                static __WRY_BINDGEN_INLINE_JS_MODULE: #krate::InlineJsModule = #krate::InlineJsModule::new(
                    #inline_js_module
                );
                #krate::inventory::submit! {
                    __WRY_BINDGEN_INLINE_JS_MODULE
                }
                __WRY_BINDGEN_INLINE_JS_MODULE.const_hash()
            };
        });
        prefix = format!("module_{{{}:x}}.", unique_ident);
    }

    // Collect type names being defined in this block
    let type_names: std::collections::HashSet<String> = program
        .types
        .iter()
        .map(|t| t.rust_name.to_string())
        .collect();

    // Collect vendor_prefixes for each type
    let vendor_prefixes: std::collections::HashMap<String, Vec<String>> = program
        .types
        .iter()
        .map(|t| {
            (
                t.rust_name.to_string(),
                t.vendor_prefixes.iter().map(|i| i.to_string()).collect(),
            )
        })
        .collect();

    // Generate type definitions
    for ty in &program.types {
        tokens.extend(generate_type(ty, krate)?);
    }

    // Generate function definitions
    for func in &program.functions {
        tokens.extend(generate_function(
            func,
            &type_names,
            &vendor_prefixes,
            krate,
            &prefix,
        )?);
    }

    // Generate static definitions
    for st in &program.statics {
        tokens.extend(generate_static(st, krate)?);
    }

    // Generate string enum definitions
    for string_enum in &program.string_enums {
        tokens.extend(generate_string_enum(string_enum, krate)?);
    }

    Ok(tokens)
}

/// Generate code for an imported type
fn generate_type(ty: &ImportType, krate: &TokenStream) -> syn::Result<TokenStream> {
    let vis = &ty.vis;
    let rust_name = &ty.rust_name;
    let derives = &ty.derives;

    // Generate the struct definition using JsValue from the configured crate
    // repr(transparent) ensures the same memory layout
    // Apply user-provided attributes (like #[derive(Debug, PartialEq, Eq)])
    // Use named struct with `obj` field to match wasm-bindgen's generated types
    let span = rust_name.span();
    let struct_def = quote_spanned! {span=>
        #(#derives)*
        #[repr(transparent)]
        #vis struct #rust_name {
            pub obj: #krate::JsValue,
        }
    };

    // Generate AsRef<JsValue> implementation
    let as_ref_impl = quote_spanned! {span=>
        impl AsRef<#krate::JsValue> for #rust_name {
            fn as_ref(&self) -> &#krate::JsValue {
                &self.obj
            }
        }

        impl #rust_name {
            /// Performs a zero-cost unchecked cast from a JsValue reference to this type.
            ///
            /// # Safety
            /// This is safe because all imported JS types are #[repr(transparent)]
            /// wrappers around JsValue with identical memory layouts.
            #[inline]
            pub fn unchecked_from_js_ref(val: &#krate::JsValue) -> &Self {
                unsafe { &*(val as *const #krate::JsValue as *const Self) }
            }
        }
    };

    // Generate From<Type> for JsValue and From<JsValue> for Type
    let into_jsvalue = quote_spanned! {span=>
        impl From<#rust_name> for #krate::JsValue {
            fn from(val: #rust_name) -> Self {
                val.obj
            }
        }

        impl From<&#rust_name> for #krate::JsValue {
            fn from(val: &#rust_name) -> Self {
                val.obj.clone()
            }
        }

        impl From<#krate::JsValue> for #rust_name {
            fn from(val: #krate::JsValue) -> Self {
                Self { obj: val }
            }
        }
    };

    // Generate Deref to JsValue - this is always safe, just field access
    let deref_impls = quote_spanned! {span=>
        impl std::ops::Deref for #rust_name {
            type Target = #krate::JsValue;
            fn deref(&self) -> &Self::Target {
                &self.obj
            }
        }
    };

    // Generate From and AsRef impls for parent types
    let mut from_parents = TokenStream::new();
    for parent in &ty.extends {
        from_parents.extend(quote_spanned! {span=>
            impl From<#rust_name> for #parent {
                fn from(val: #rust_name) -> #parent {
                    #parent { obj: val.obj }
                }
            }

            impl From<&#rust_name> for #parent {
                fn from(val: &#rust_name) -> #parent {
                    #parent { obj: val.obj.clone() }
                }
            }

            impl AsRef<#parent> for #rust_name {
                #[inline]
                fn as_ref(&self) -> &#parent {
                    #parent::unchecked_from_js_ref(self.as_ref())
                }
            }
        });
    }

    // Generate TypeConstructor implementation
    // All JS types use HeapRefType since they're references to JS heap objects
    let type_constructor_impl = quote_spanned! {span=>
        impl #krate::TypeConstructor for #rust_name {
            fn create_type_instance() -> String {
                <#krate::JsValue as #krate::TypeConstructor>::create_type_instance()
            }
        }
    };

    // Generate BinaryEncode implementation
    let binary_encode_impl = quote_spanned! {span=>
        impl #krate::BinaryEncode for #rust_name {
            fn encode(self, encoder: &mut #krate::EncodedData) {
                self.obj.encode(encoder);
            }
        }

        impl #krate::BinaryEncode for &#rust_name {
            fn encode(self, encoder: &mut #krate::EncodedData) {
                (&self.obj).encode(encoder);
            }
        }
    };

    // Generate BinaryDecode implementation
    let binary_decode_impl = quote_spanned! {span=>
        impl #krate::BinaryDecode for #rust_name {
            fn decode(decoder: &mut #krate::DecodedData) -> Result<Self, #krate::DecodeError> {
                #krate::JsValue::decode(decoder).map(|v| Self { obj: v })
            }
        }
    };

    // Generate BatchableResult implementation
    let batchable_impl = quote_spanned! {span=>
        impl #krate::BatchableResult for #rust_name {
            fn needs_flush() -> bool {
                false
            }

            fn batched_placeholder(batch: &mut #krate::batch::BatchState) -> Self {
                Self { obj: <#krate::JsValue as #krate::BatchableResult>::batched_placeholder(batch) }
            }
        }
    };

    // Generate JsCast implementation
    let jscast_impl = quote_spanned! {span=>
        impl #krate::JsCast for #rust_name {
            fn instanceof(val: &#krate::JsValue) -> bool {
                // For now, always return false - proper instanceof requires JS runtime check
                let _ = val;
                false
            }

            fn unchecked_from_js(val: #krate::JsValue) -> Self {
                Self { obj: val }
            }

            fn unchecked_from_js_ref(val: &#krate::JsValue) -> &Self {
                // SAFETY: #[repr(transparent)] guarantees same layout
                unsafe { &*(val as *const #krate::JsValue as *const Self) }
            }
        }
    };

    Ok(quote_spanned! {span=>
        #struct_def
        #as_ref_impl
        #into_jsvalue
        #deref_impls
        #from_parents
        #type_constructor_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_impl
        #jscast_impl
    })
}

/// Generate code for an imported function
fn generate_function(
    func: &ImportFunction,
    type_names: &std::collections::HashSet<String>,
    vendor_prefixes: &std::collections::HashMap<String, Vec<String>>,
    krate: &TokenStream,
    prefix: &str,
) -> syn::Result<TokenStream> {
    let vis = &func.vis;
    let rust_name = &func.rust_name;
    let span = rust_name.span();

    // Generate unique function name for registry
    let registry_name = match &func.kind {
        ImportFunctionKind::Normal => {
            if let Some(ref ns) = func.js_namespace {
                format!("{}::{}", ns.join("."), func.rust_name)
            } else {
                func.rust_name.to_string()
            }
        }
        ImportFunctionKind::Method { .. }
        | ImportFunctionKind::Getter { .. }
        | ImportFunctionKind::Setter { .. } => {
            let class = func.js_class.as_deref().unwrap_or("global");
            format!("{}::{}", class, rust_name)
        }
        ImportFunctionKind::Constructor { class } => format!("{}::new", class),
        ImportFunctionKind::StaticMethod { class } => format!("{}::{}", class, rust_name),
    };

    // Generate argument lists
    let args = generate_args(func, krate)?;
    let fn_params = &args.fn_params;
    let fn_types = &args.fn_types;
    let call_values = &args.call_values;
    let type_constructors = &args.type_constructors;

    // Generate return type
    let ret_type = match &func.ret {
        Some(ty) => quote_spanned! {span=> #ty },
        None => quote_spanned! {span=> () },
    };

    // Generate return type constructor
    let ret_type_constructor = match &func.ret {
        Some(_ty) => {
            quote_spanned! {span=> <#ret_type as #krate::TypeConstructor<_>>::create_type_instance() }
        }
        None => quote_spanned! {span=> "new window.NullType()".to_string() },
    };

    // For non-inline_js, generate a simple closure that returns a constant string
    let js_code = generate_js_code(func, vendor_prefixes, prefix);
    let js_code_str = js_code.to_arrow_function();

    // Generate the function body
    let func_body = quote_spanned! {span=>
        static __SPEC: #krate::JsFunctionSpec = #krate::JsFunctionSpec::new(
            || format!(#js_code_str),
            || (#type_constructors, #ret_type_constructor),
        );

        #krate::inventory::submit! {
            __SPEC
        }

        // Look up the function at runtime
        let func: #krate::JSFunction<fn(#fn_types) -> #ret_type> =
            #krate::FUNCTION_REGISTRY
                .get_function(__SPEC)
                .expect(concat!("Function not found: ", #registry_name));

        // Call the function
        func.call(#call_values)
    };

    // Generate the full function based on kind
    match &func.kind {
        ImportFunctionKind::Normal => {
            // Check if this function has a single-element js_namespace that matches a type
            // defined in this extern block. If so, generate as a static method to avoid collisions.
            if let Some(ns) = &func.js_namespace {
                if ns.len() == 1 && type_names.contains(&ns[0]) {
                    let class_ident = format_ident!("{}", &ns[0]);
                    return Ok(quote_spanned! {span=>
                        impl #class_ident {
                            #vis fn #rust_name(#fn_params) -> #ret_type {
                                #func_body
                            }
                        }
                    });
                }
            }
            Ok(quote_spanned! {span=>
                #vis fn #rust_name(#fn_params) -> #ret_type {
                    #func_body
                }
            })
        }
        ImportFunctionKind::Method { receiver }
        | ImportFunctionKind::Getter { receiver, .. }
        | ImportFunctionKind::Setter { receiver, .. } => {
            // Extract the type name from the receiver
            let receiver_type = extract_type_name(receiver)?;

            // Build method signature with optional additional args
            let method_args = if fn_params.is_empty() {
                quote_spanned! {span=> &self }
            } else {
                quote_spanned! {span=> &self, #fn_params }
            };

            Ok(quote_spanned! {span=>
                impl #receiver_type {
                    #vis fn #rust_name(#method_args) -> #ret_type {
                        #func_body
                    }
                }
            })
        }
        ImportFunctionKind::Constructor { class } => {
            let class_ident = format_ident!("{}", class);
            // Use the actual return type (may be Result<T, JsValue> for catch constructors)
            Ok(quote_spanned! {span=>
                impl #class_ident {
                    #vis fn #rust_name(#fn_params) -> #ret_type {
                        #func_body
                    }
                }
            })
        }
        ImportFunctionKind::StaticMethod { class } => {
            let class_ident = format_ident!("{}", class);
            Ok(quote_spanned! {span=>
                impl #class_ident {
                    #vis fn #rust_name(#fn_params) -> #ret_type {
                        #func_body
                    }
                }
            })
        }
    }
}

/// Generate vendor-prefixed constructor fallback code
/// E.g., for class "MyApi" with prefixes ["webkit", "moz"], generates:
/// (typeof MyApi !== 'undefined' ? MyApi : (typeof webkitMyApi !== 'undefined' ? webkitMyApi : (typeof mozMyApi !== 'undefined' ? mozMyApi : undefined)))
fn generate_vendor_prefixed_constructor(class: &str, prefixes: &[String], prefix: &str) -> String {
    // Start with the base class name (no prefix)
    let mut result = format!("(typeof {prefix}{class} !== 'undefined' ? {prefix}{class} : ");

    // Add each vendor prefix
    for (i, vendor_prefix) in prefixes.iter().enumerate() {
        let prefixed_class = format!("{}{}", vendor_prefix, class);
        if i == prefixes.len() - 1 {
            // Last one - end with undefined if none found
            result.push_str(&format!(
                "(typeof {prefix}{} !== 'undefined' ? {prefix}{} : undefined)",
                prefixed_class, prefixed_class
            ));
        } else {
            result.push_str(&format!(
                "(typeof {prefix}{} !== 'undefined' ? {prefix}{} : ",
                prefixed_class, prefixed_class
            ));
        }
    }

    // Close all the parentheses
    result.push(')');
    result
}

/// Generate JavaScript code for the function
fn generate_js_code(
    func: &ImportFunction,
    vendor_prefixes: &std::collections::HashMap<String, Vec<String>>,
    prefix: &str,
) -> JsCode {
    let js_name = &func.js_name;

    let prefix = if let Some(ns) = &func.js_namespace {
        if !ns.is_empty() {
            format!("{prefix}{}.", ns.join("."))
        } else {
            prefix.to_string()
        }
    } else {
        prefix.to_string()
    };

    let (params, body) = match &func.kind {
        ImportFunctionKind::Normal => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            (
                format!("({})", args_str),
                format!("{prefix}{}({})", js_name, args_str),
            )
        }
        ImportFunctionKind::Method { .. } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            if args.is_empty() {
                ("(obj)".to_string(), format!("obj.{}()", js_name))
            } else {
                (
                    format!("(obj, {})", args_str),
                    format!("obj.{}({})", js_name, args_str),
                )
            }
        }
        ImportFunctionKind::Getter { property, .. } => {
            ("(obj)".to_string(), format!("obj.{}", property))
        }
        ImportFunctionKind::Setter { property, .. } => (
            "(obj, value)".to_string(),
            format!("{{{{ obj.{} = value; }}}}", property),
        ),
        ImportFunctionKind::Constructor { class } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");

            // Check if this type has vendor prefixes
            let body = if let Some(prefixes) = vendor_prefixes.get(class) {
                if !prefixes.is_empty() {
                    // Generate vendor-prefixed fallback code
                    let constructor_expr =
                        generate_vendor_prefixed_constructor(class, prefixes, &prefix);
                    format!("new ({})({})", constructor_expr, args_str)
                } else {
                    format!("new {prefix}{}({})", class, args_str)
                }
            } else {
                format!("new {prefix}{}({})", class, args_str)
            };

            (format!("({})", args_str), body)
        }
        ImportFunctionKind::StaticMethod { class } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            (
                format!("({})", args_str),
                format!("{prefix}{}.{}({})", class, js_name, args_str),
            )
        }
    };

    // Wrap in try-catch if catch attribute is present
    let body = if func.catch {
        wrap_body_with_try_catch(&body)
    } else {
        body
    };

    JsCode { params, body }
}

/// Wrap JavaScript body in try-catch block for error handling
fn wrap_body_with_try_catch(body: &str) -> String {
    // Wrap the body in try-catch and return Result-like object
    format!(
        "{{{{ try {{{{ return {{{{ ok: {} }}}}; }}}} catch(e) {{{{ return {{{{ err: e }}}}; }}}} }}}}",
        body
    )
}

/// JavaScript function code parts
struct JsCode {
    /// Function parameters (e.g., "(arg1, arg2)" or "(obj, arg1, arg2)")
    params: String,
    /// Function body (e.g., "obj.method(arg1, arg2)" or "new Class(arg1)")
    body: String,
}

impl JsCode {
    /// Convert to a complete JavaScript arrow function
    fn to_arrow_function(&self) -> String {
        format!("{} => {}", self.params, self.body)
    }
}

/// Generated argument information
struct GeneratedArgs {
    /// Function parameter declarations: `arg1: T1, arg2: T2`
    fn_params: TokenStream,
    /// Just the types for fn pointer: `T1, T2`
    fn_types: TokenStream,
    /// Values to pass to call: `&self.obj, arg1, arg2`
    call_values: TokenStream,
    /// Type constructor expressions
    type_constructors: TokenStream,
}

/// Generate argument lists
fn generate_args(func: &ImportFunction, krate: &TokenStream) -> syn::Result<GeneratedArgs> {
    let mut fn_params = Vec::new();
    let mut fn_types = Vec::new();
    let mut call_values = Vec::new();
    let mut type_constructors = Vec::new();
    let span = func.rust_name.span();

    // For methods, add self as first call arg (but not as fn param since we use &self)
    match &func.kind {
        ImportFunctionKind::Method { .. }
        | ImportFunctionKind::Getter { .. }
        | ImportFunctionKind::Setter { .. } => {
            fn_types.push(quote_spanned! {span=> &#krate::JsValue });
            call_values.push(quote_spanned! {span=> &self.obj });
            type_constructors.push(
                quote_spanned! {span=> <&#krate::JsValue as #krate::TypeConstructor<_>>::create_type_instance() },
            );
        }
        _ => {}
    }

    // Add explicit arguments
    for arg in &func.arguments {
        let name = &arg.name;
        let ty = &arg.ty;
        fn_params.push(quote_spanned! {span=> #name: #ty });
        fn_types.push(quote_spanned! {span=> #ty });
        call_values.push(quote_spanned! {span=> #name });
        type_constructors.push(
            quote_spanned! {span=> <#ty as #krate::TypeConstructor<_>>::create_type_instance() },
        );
    }

    let fn_params_tokens = if fn_params.is_empty() {
        quote_spanned! {span=>}
    } else {
        quote_spanned! {span=> #(#fn_params),* }
    };

    let fn_types_tokens = if fn_types.is_empty() {
        quote_spanned! {span=>}
    } else {
        quote_spanned! {span=> #(#fn_types),* }
    };

    let call_values_tokens = if call_values.is_empty() {
        quote_spanned! {span=>}
    } else {
        quote_spanned! {span=> #(#call_values),* }
    };

    let type_constructors_tokens = quote_spanned! {span=>
        std::vec![#(#type_constructors),*]
    };

    Ok(GeneratedArgs {
        fn_params: fn_params_tokens,
        fn_types: fn_types_tokens,
        call_values: call_values_tokens,
        type_constructors: type_constructors_tokens,
    })
}

/// Extract the type name from a syn::Type (handles &Type and Type)
fn extract_type_name(ty: &syn::Type) -> syn::Result<&syn::Ident> {
    match ty {
        syn::Type::Reference(r) => extract_type_name(&r.elem),
        syn::Type::Path(p) => p
            .path
            .get_ident()
            .ok_or_else(|| syn::Error::new_spanned(ty, "expected simple type name")),
        _ => Err(syn::Error::new_spanned(ty, "unsupported receiver type")),
    }
}

/// Generate code for an imported static
fn generate_static(st: &ImportStatic, krate: &TokenStream) -> syn::Result<TokenStream> {
    let vis = &st.vis;
    let rust_name = &st.rust_name;
    let ty = &st.ty;
    let span = rust_name.span();

    // Generate registry name for the static accessor
    let registry_name = format!("__static_{}", rust_name);

    // Generate JavaScript code to access the static
    let js_code = generate_static_js_code(st);

    // Generate the type constructor for the return type
    let ret_type_constructor =
        quote_spanned! {span=> <#ty as #krate::TypeConstructor<_>>::create_type_instance() };

    assert!(st.thread_local_v2);

    // Generate a lazily-initialized thread-local static
    Ok(quote_spanned! {span=>
        #vis static #rust_name: #krate::JsThreadLocal<#ty> = {
            static __SPEC: #krate::JsFunctionSpec = #krate::JsFunctionSpec::new(
                || format!(#js_code),
                || (std::vec![] as std::vec::Vec<String>, #ret_type_constructor),
            );

            #krate::inventory::submit! {
                __SPEC
            }

            fn init() -> #ty {
                // Look up the accessor function at runtime
                let func: #krate::JSFunction<fn() -> #ty> =
                    #krate::FUNCTION_REGISTRY
                        .get_function(__SPEC)
                        .expect(concat!("Static accessor not found: ", #registry_name));

                // Call the accessor to get the value
                func.call()
            }
            #krate::__wry_bindgen_thread_local!(#ty = init())
        };
    })
}

/// Generate JavaScript code to access a static value
fn generate_static_js_code(st: &ImportStatic) -> String {
    let js_name = &st.js_name;

    // Build the full path with namespace if present
    if let Some(ref namespace) = st.js_namespace {
        let namespace_path = namespace.join(".");
        format!("() => {}.{}", namespace_path, js_name)
    } else {
        format!("() => {}", js_name)
    }
}

/// Generate code for a string enum
fn generate_string_enum(string_enum: &StringEnum, krate: &TokenStream) -> syn::Result<TokenStream> {
    let vis = &string_enum.vis;
    let enum_name = &string_enum.name;
    let variants = &string_enum.variants;
    let variant_values = &string_enum.variant_values;
    let rust_attrs = &string_enum.rust_attrs;
    let span = enum_name.span();

    let variant_count = variants.len();
    let variant_indices: Vec<u32> = (0..variant_count as u32).collect();

    let invalid_to_str_msg = format!(
        "Converting an invalid string enum ({}) back to a string is currently not supported",
        enum_name
    );

    // Generate variant paths for match arms (EnumName::VariantName)
    let variant_paths: Vec<TokenStream> = variants
        .iter()
        .map(|v| quote_spanned!(span=> #enum_name::#v))
        .collect();

    // Generate the enum definition with repr(u32)
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

    // Generate helper methods (from_str, to_str, from_js_value)
    let impl_methods = quote! {
        #[automatically_derived]
        impl #enum_name {
            /// Convert a string to this enum variant.
            pub fn from_str(s: &str) -> Option<#enum_name> {
                match s {
                    #(#variant_values => Some(#variant_paths),)*
                    _ => None,
                }
            }

            /// Convert this enum variant to its string representation.
            pub fn to_str(&self) -> &'static str {
                match self {
                    #(#variant_paths => #variant_values,)*
                    #enum_name::__Invalid => panic!(#invalid_to_str_msg),
                }
            }

            /// Convert a JsValue (if it's a string) to this enum variant.
            #vis fn from_js_value(obj: &#krate::JsValue) -> Option<#enum_name> {
                obj.as_string().and_then(|s| Self::from_str(&s))
            }
        }
    };

    // Generate TypeConstructor implementation
    // String enums use StringEnumType with a lookup array for efficient u32 encoding
    let type_constructor_impl = quote! {
        impl #krate::TypeConstructor for #enum_name {
            fn create_type_instance() -> String {
                format!("new window.StringEnumType([{}])",
                    vec![#(#variant_values),*].join(", "))
            }
        }
    };

    // Generate BinaryEncode implementation - encode as u32 discriminant
    let binary_encode_impl = quote! {
        impl #krate::BinaryEncode for #enum_name {
            fn encode(self, encoder: &mut #krate::EncodedData) {
                encoder.push_u32(self as u32);
            }
        }
    };

    // Generate BinaryDecode implementation - decode u32 to variant
    let binary_decode_impl = quote! {
        impl #krate::BinaryDecode for #enum_name {
            fn decode(decoder: &mut #krate::DecodedData) -> Result<Self, #krate::DecodeError> {
                let discriminant = decoder.take_u32()?;
                match discriminant {
                    #(#variant_indices => Ok(#variant_paths),)*
                    _ => Ok(#enum_name::__Invalid),
                }
            }
        }
    };

    // Generate BatchableResult implementation
    let batchable_impl = quote! {
        impl #krate::BatchableResult for #enum_name {
            fn needs_flush() -> bool {
                true
            }

            fn batched_placeholder(_batch: &mut #krate::batch::BatchState) -> Self {
                unreachable!("needs_flush types should never call batched_placeholder")
            }
        }
    };

    // Generate From<EnumName> for JsValue
    let into_jsvalue_impl = quote! {
        #[automatically_derived]
        impl From<#enum_name> for #krate::JsValue {
            fn from(val: #enum_name) -> Self {
                #krate::JsValue::from_str(val.to_str())
            }
        }
    };

    Ok(quote! {
        #enum_def
        #impl_methods
        #type_constructor_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_impl
        #into_jsvalue_impl
    })
}
