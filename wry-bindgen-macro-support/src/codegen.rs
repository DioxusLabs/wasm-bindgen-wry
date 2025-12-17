//! Code generation for wasm_bindgen macro
//!
//! This module generates Rust code that uses the wry-bindgen runtime
//! and inventory-based function registration.

use crate::ast::{ImportFunction, ImportFunctionKind, ImportStatic, ImportType, Program};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate code for the entire program
pub fn generate(program: &Program) -> syn::Result<TokenStream> {
    let mut tokens = TokenStream::new();

    // Generate type definitions
    for ty in &program.types {
        tokens.extend(generate_type(ty)?);
    }

    // Generate function definitions
    for func in &program.functions {
        tokens.extend(generate_function(func)?);
    }

    // Generate static definitions
    for st in &program.statics {
        tokens.extend(generate_static(st)?);
    }

    Ok(tokens)
}

/// Generate code for an imported type
fn generate_type(ty: &ImportType) -> syn::Result<TokenStream> {
    let vis = &ty.vis;
    let rust_name = &ty.rust_name;
    let _js_name = &ty.js_name;

    // Generate the struct definition using wry_bindgen::JsValue
    // repr(transparent) ensures the same memory layout
    let struct_def = quote! {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        #vis struct #rust_name(wry_bindgen::JsValue);
    };

    // Generate AsRef<JsValue> implementation
    let as_ref_impl = quote! {
        impl AsRef<wry_bindgen::JsValue> for #rust_name {
            fn as_ref(&self) -> &wry_bindgen::JsValue {
                &self.0
            }
        }

        impl #rust_name {
            /// Performs a zero-cost unchecked cast from a JsValue reference to this type.
            ///
            /// # Safety
            /// This is safe because all imported JS types are #[repr(transparent)]
            /// wrappers around JsValue with identical memory layouts.
            #[inline]
            pub fn unchecked_from_js_ref(val: &wry_bindgen::JsValue) -> &Self {
                unsafe { &*(val as *const wry_bindgen::JsValue as *const Self) }
            }
        }
    };

    // Generate From<Type> for JsValue
    let into_jsvalue = quote! {
        impl From<#rust_name> for wry_bindgen::JsValue {
            fn from(val: #rust_name) -> Self {
                val.0
            }
        }
    };

    // Generate Deref to JsValue - this is always safe, just field access
    let deref_impls = quote! {
        impl std::ops::Deref for #rust_name {
            type Target = wry_bindgen::JsValue;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };

    // Generate From and AsRef impls for parent types
    let mut from_parents = TokenStream::new();
    for parent in &ty.extends {
        from_parents.extend(quote! {
            impl From<#rust_name> for #parent {
                fn from(val: #rust_name) -> #parent {
                    #parent(val.0)
                }
            }

            impl From<&#rust_name> for #parent {
                fn from(val: &#rust_name) -> #parent {
                    #parent(val.0.clone())
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
    let type_constructor_impl = quote! {
        impl wry_bindgen::TypeConstructor for #rust_name {
            fn create_type_instance() -> String {
                <wry_bindgen::JsValue as wry_bindgen::TypeConstructor>::create_type_instance()
            }
        }
    };

    // Generate BinaryEncode implementation
    let binary_encode_impl = quote! {
        impl wry_bindgen::BinaryEncode for #rust_name {
            fn encode(self, encoder: &mut wry_bindgen::EncodedData) {
                self.0.encode(encoder);
            }
        }
    };

    // Generate BinaryDecode implementation
    let binary_decode_impl = quote! {
        impl wry_bindgen::BinaryDecode for #rust_name {
            fn decode(decoder: &mut wry_bindgen::DecodedData) -> Result<Self, wry_bindgen::DecodeError> {
                wry_bindgen::JsValue::decode(decoder).map(Self)
            }
        }
    };

    // Generate BatchableResult implementation
    let batchable_impl = quote! {
        impl wry_bindgen::BatchableResult for #rust_name {
            fn needs_flush() -> bool {
                false
            }

            fn batched_placeholder(batch: &mut wry_bindgen::batch::BatchState) -> Self {
                Self(<wry_bindgen::JsValue as wry_bindgen::BatchableResult>::batched_placeholder(batch))
            }
        }
    };

    Ok(quote! {
        #struct_def
        #as_ref_impl
        #into_jsvalue
        #deref_impls
        #from_parents
        #type_constructor_impl
        #binary_encode_impl
        #binary_decode_impl
        #batchable_impl
    })
}

/// Generate code for an imported function
fn generate_function(func: &ImportFunction) -> syn::Result<TokenStream> {
    let vis = &func.vis;
    let rust_name = &func.rust_name;

    // Generate unique function name for registry
    let registry_name = match &func.kind {
        ImportFunctionKind::Normal => func.rust_name.to_string(),
        ImportFunctionKind::Method { .. }
        | ImportFunctionKind::Getter { .. }
        | ImportFunctionKind::Setter { .. } => {
            let class = func.js_class.as_deref().unwrap_or("global");
            format!("{}::{}", class, rust_name)
        }
        ImportFunctionKind::Constructor { class } => format!("{}::new", class),
        ImportFunctionKind::StaticMethod { class } => format!("{}::{}", class, rust_name),
    };

    // Generate JavaScript code based on function kind
    let js_code = generate_js_code(func);

    // Generate argument lists
    let args = generate_args(func)?;
    let fn_params = &args.fn_params;
    let fn_types = &args.fn_types;
    let call_values = &args.call_values;
    let type_constructors = &args.type_constructors;

    // Generate return type
    let ret_type = match &func.ret {
        Some(ty) => quote! { #ty },
        None => quote! { () },
    };

    // Generate return type constructor
    let ret_type_constructor = match &func.ret {
        Some(_ty) => quote! { <#ret_type as wry_bindgen::TypeConstructor<_>>::create_type_instance() },
        None => quote! { "new window.NullType()".to_string() },
    };

    // Generate the inline_js option
    let js_name_str = &func.js_name;
    let inline_js_option = if let Some(inline_js) = func.inline_js.as_ref() {
        quote! { Some(crate::InlineJsModule::new(#inline_js, #js_name_str)) }
    } else {
        quote! { None }
    };

    // Generate the function body
    let func_body = quote! {
        inventory::submit! {
            crate::JsFunctionSpec::new(
                #registry_name,
                #js_code,
                || (#type_constructors, #ret_type_constructor),
                #inline_js_option
            )
        }

        // Look up the function at runtime
        let func: wry_bindgen::JSFunction<fn(#fn_types) -> #ret_type> =
            crate::FUNCTION_REGISTRY
                .get_function(#registry_name)
                .expect(concat!("Function not found: ", #registry_name));

        // Call the function
        func.call(#call_values)
    };

    // Generate the full function based on kind
    match &func.kind {
        ImportFunctionKind::Normal => {
            Ok(quote! {
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
                quote! { &self }
            } else {
                quote! { &self, #fn_params }
            };

            Ok(quote! {
                impl #receiver_type {
                    #vis fn #rust_name(#method_args) -> #ret_type {
                        #func_body
                    }
                }
            })
        }
        ImportFunctionKind::Constructor { class } => {
            let class_ident = format_ident!("{}", class);
            Ok(quote! {
                impl #class_ident {
                    #vis fn #rust_name(#fn_params) -> #class_ident {
                        #func_body
                    }
                }
            })
        }
        ImportFunctionKind::StaticMethod { class } => {
            let class_ident = format_ident!("{}", class);
            Ok(quote! {
                impl #class_ident {
                    #vis fn #rust_name(#fn_params) -> #ret_type {
                        #func_body
                    }
                }
            })
        }
    }
}

/// Generate JavaScript code for the function
fn generate_js_code(func: &ImportFunction) -> String {
    // If inline_js is present, reference the pre-loaded module from window.__wryModules
    if func.inline_js.is_some() {
        let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
        let args_str = args.join(", ");
        let js_name = &func.js_name;
        let registry_name = &func.rust_name;
        // Reference the pre-loaded module export
        return format!(
            "({}) => window.__wryModules[\"{}\"].{}({})",
            args_str,
            registry_name,
            js_name,
            args_str
        );
    }

    let js_name = &func.js_name;

    match &func.kind {
        ImportFunctionKind::Normal => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            format!("({}) => {}({})", args_str, js_name, args_str)
        }
        ImportFunctionKind::Method { .. } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            if args.is_empty() {
                format!("(obj) => obj.{}()", js_name)
            } else {
                format!("(obj, {}) => obj.{}({})", args_str, js_name, args_str)
            }
        }
        ImportFunctionKind::Getter { property, .. } => {
            format!("(obj) => obj.{}", property)
        }
        ImportFunctionKind::Setter { property, .. } => {
            format!("(obj, value) => {{ obj.{} = value; }}", property)
        }
        ImportFunctionKind::Constructor { class } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            format!("({}) => new {}({})", args_str, class, args_str)
        }
        ImportFunctionKind::StaticMethod { class } => {
            let args: Vec<_> = func.arguments.iter().map(|a| a.name.to_string()).collect();
            let args_str = args.join(", ");
            format!("({}) => {}.{}({})", args_str, class, js_name, args_str)
        }
    }
}

/// Generated argument information
struct GeneratedArgs {
    /// Function parameter declarations: `arg1: T1, arg2: T2`
    fn_params: TokenStream,
    /// Just the types for fn pointer: `T1, T2`
    fn_types: TokenStream,
    /// Values to pass to call: `self.0.clone(), arg1, arg2`
    call_values: TokenStream,
    /// Type constructor expressions
    type_constructors: TokenStream,
}

/// Generate argument lists
fn generate_args(func: &ImportFunction) -> syn::Result<GeneratedArgs> {
    let mut fn_params = Vec::new();
    let mut fn_types = Vec::new();
    let mut call_values = Vec::new();
    let mut type_constructors = Vec::new();

    // For methods, add self as first call arg (but not as fn param since we use &self)
    match &func.kind {
        ImportFunctionKind::Method { .. }
        | ImportFunctionKind::Getter { .. }
        | ImportFunctionKind::Setter { .. } => {
            fn_types.push(quote! { wry_bindgen::JsValue });
            call_values.push(quote! { self.0.clone() });
            type_constructors.push(quote! { <wry_bindgen::JsValue as wry_bindgen::TypeConstructor<_>>::create_type_instance() });
        }
        _ => {}
    }

    // Add explicit arguments
    for arg in &func.arguments {
        let name = &arg.name;
        let ty = &arg.ty;
        fn_params.push(quote! { #name: #ty });
        fn_types.push(quote! { #ty });
        call_values.push(quote! { #name });
        type_constructors.push(quote! { <#ty as wry_bindgen::TypeConstructor<_>>::create_type_instance() });
    }

    let fn_params_tokens = if fn_params.is_empty() {
        quote! {}
    } else {
        quote! { #(#fn_params),* }
    };

    let fn_types_tokens = if fn_types.is_empty() {
        quote! {}
    } else {
        quote! { #(#fn_types),* }
    };

    let call_values_tokens = if call_values.is_empty() {
        quote! {}
    } else {
        quote! { #(#call_values),* }
    };

    let type_constructors_tokens = quote! {
        vec![#(#type_constructors),*]
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
        syn::Type::Path(p) => {
            p.path.get_ident().ok_or_else(|| {
                syn::Error::new_spanned(ty, "expected simple type name")
            })
        }
        _ => Err(syn::Error::new_spanned(ty, "unsupported receiver type")),
    }
}

/// Generate code for an imported static
fn generate_static(st: &ImportStatic) -> syn::Result<TokenStream> {
    let vis = &st.vis;
    let rust_name = &st.rust_name;
    let ty = &st.ty;

    // Generate registry name for the static accessor
    let registry_name = format!("__static_{}", rust_name);

    // Generate JavaScript code to access the static
    let js_code = generate_static_js_code(st);

    // Generate the type constructor for the return type
    let ret_type_constructor = quote! { <#ty as wry_bindgen::TypeConstructor<_>>::create_type_instance() };

    if st.thread_local_v2 {
        // Generate a lazily-initialized thread-local static
        Ok(quote! {
            inventory::submit! {
                crate::JsFunctionSpec::new(
                    #registry_name,
                    #js_code,
                    || (vec![] as Vec<String>, #ret_type_constructor),
                    None
                )
            }

            #vis static #rust_name: wry_bindgen::JsThreadLocal<#ty> = {
                fn init() -> #ty {
                    // Look up the accessor function at runtime
                    let func: wry_bindgen::JSFunction<fn() -> #ty> =
                        crate::FUNCTION_REGISTRY
                            .get_function(#registry_name)
                            .expect(concat!("Static accessor not found: ", #registry_name));

                    // Call the accessor to get the value
                    func.call()
                }
                wry_bindgen::__wry_bindgen_thread_local!(#ty = init())
            };
        })
    } else {
        // For non-thread-local statics, generate a regular function accessor
        // This matches the behavior of wasm-bindgen without thread_local_v2 attribute
        Ok(quote! {
            inventory::submit! {
                crate::JsFunctionSpec::new(
                    #registry_name,
                    #js_code,
                    || (vec![] as Vec<String>, #ret_type_constructor),
                    None
                )
            }

            #vis fn #rust_name() -> #ty {
                // Look up the accessor function at runtime
                let func: wry_bindgen::JSFunction<fn() -> #ty> =
                    crate::FUNCTION_REGISTRY
                        .get_function(#registry_name)
                        .expect(concat!("Static accessor not found: ", #registry_name));

                // Call the accessor to get the value
                func.call()
            }
        })
    }
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
