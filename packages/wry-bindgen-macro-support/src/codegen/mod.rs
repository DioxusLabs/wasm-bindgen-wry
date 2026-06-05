//! Code generation for wasm_bindgen macro
//!
//! This module generates Rust code that uses the wry-bindgen runtime
//! and inventory-based function registration.

mod common;
mod dynamic_union;
mod erasure;
mod exports;
mod imports;
mod js;
mod numeric_enum;
mod statics;
mod string_enum;
mod types;

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering},
};

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote_spanned};
use wasm_bindgen_macro_support::{ParseOutput, ast};

use dynamic_union::generate_dynamic_union;
use exports::{
    generate_export_function, generate_export_method, generate_export_struct,
    generate_main_function,
};
use imports::generate_function;
use numeric_enum::generate_numeric_enum;
use statics::{generate_static, generate_static_string};
use string_enum::generate_string_enum;
use types::generate_type;

static NEXT_MODULE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Hash, PartialEq, Eq)]
enum ModuleKey {
    Named(String),
    Raw(String),
    Inline(usize),
}

impl ModuleKey {
    fn from_module(module: &ast::ImportModule) -> Self {
        match module {
            ast::ImportModule::Named(path, _) => Self::Named(path.clone()),
            ast::ImportModule::RawNamed(path, _) => Self::Raw(path.clone()),
            ast::ImportModule::Inline(index) => Self::Inline(*index),
        }
    }
}

fn module_span(module: &ast::ImportModule) -> proc_macro2::Span {
    match module {
        ast::ImportModule::Named(_, span) | ast::ImportModule::RawNamed(_, span) => *span,
        ast::ImportModule::Inline(_) => proc_macro2::Span::call_site(),
    }
}

fn module_spec_expr(
    module: &ast::ImportModule,
    inline_js: &[String],
    krate: &TokenStream,
) -> syn::Result<TokenStream> {
    Ok(match module {
        ast::ImportModule::Named(module_path, span) => {
            // Match upstream wasm-bindgen's `module` resolution: a leading `/` or a bare
            // path (e.g. "tests/wasm/foo.js") is relative to the crate root
            // (`CARGO_MANIFEST_DIR`). `./` and `../` are relative to the source file.
            let include_expr = if module_path.starts_with('/') {
                quote_spanned! {*span=> include_str!(concat!(env!("CARGO_MANIFEST_DIR"), #module_path)) }
            } else if module_path.starts_with("./") || module_path.starts_with("../") {
                quote_spanned! {*span=> include_str!(#module_path) }
            } else {
                quote_spanned! {*span=> include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #module_path)) }
            };
            quote_spanned! {*span=> #krate::__rt::JsModuleSpec::new(#include_expr) }
        }
        ast::ImportModule::RawNamed(raw_module, span) => {
            quote_spanned! {*span=> #krate::__rt::JsModuleSpec::raw(#raw_module) }
        }
        ast::ImportModule::Inline(index) => {
            let source = inline_js.get(*index).ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "invalid upstream inline_js index",
                )
            })?;
            quote_spanned! {proc_macro2::Span::call_site()=> #krate::__rt::JsModuleSpec::new(#source) }
        }
    })
}

fn module_ident(key: &ModuleKey) -> Ident {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let id = NEXT_MODULE_ID.fetch_add(1, Ordering::Relaxed);
    format_ident!("__WRY_BINDGEN_JS_MODULE_{}_{}", hasher.finish(), id)
}

pub(crate) fn generate(output: &ParseOutput) -> syn::Result<TokenStream> {
    let program = &output.program;
    let mut tokens = TokenStream::new();
    tokens.extend(output.tokens.clone());

    let krate = program.wasm_bindgen.to_token_stream();
    let configured_wasm_bindgen_futures = program.wasm_bindgen_futures.to_token_stream();
    let configured_js_sys = program.js_sys.to_token_stream();
    // Match upstream wasm-bindgen's async codegen switch. By default async
    // code goes through `wasm_bindgen_futures` and its `js_sys` re-export, so
    // callers don't need a direct `js-sys` dependency. Crates that opt in with
    // `--cfg=wasm_bindgen_use_js_sys` use `js_sys::futures` and `js_sys::Promise`.
    let use_js_sys_futures = ast::use_js_sys_futures();
    let futures = if use_js_sys_futures {
        quote_spanned! { proc_macro2::Span::call_site()=> #configured_js_sys::futures }
    } else {
        configured_wasm_bindgen_futures.clone()
    };
    let js_sys = if use_js_sys_futures {
        configured_js_sys
    } else {
        quote_spanned! { proc_macro2::Span::call_site()=> #configured_wasm_bindgen_futures::js_sys }
    };

    let mut module_bindings = HashMap::<ModuleKey, Ident>::new();
    for import in &program.imports {
        let Some(module) = &import.module else {
            continue;
        };
        let key = ModuleKey::from_module(module);
        if module_bindings.contains_key(&key) {
            continue;
        }

        let ident = module_ident(&key);
        let span = module_span(module);
        let spec_expr = module_spec_expr(module, &program.inline_js, &krate)?;
        tokens.extend(quote_spanned! {span=>
            static #ident: #krate::__rt::JsModuleSpec = #spec_expr;
        });
        module_bindings.insert(key, ident);
    }

    let type_names: HashSet<String> = program
        .imports
        .iter()
        .filter_map(|import| match &import.kind {
            ast::ImportKind::Type(ty) => Some(ty.rust_name.to_string()),
            _ => None,
        })
        .collect();
    let type_generics: HashMap<String, syn::Generics> = program
        .imports
        .iter()
        .filter_map(|import| match &import.kind {
            ast::ImportKind::Type(ty) => Some((ty.rust_name.to_string(), ty.generics.clone())),
            _ => None,
        })
        .collect();
    let mut vendor_prefixes: HashMap<String, Vec<String>> = HashMap::new();
    for import in &program.imports {
        let ast::ImportKind::Type(ty) = &import.kind else {
            continue;
        };
        let prefixes: Vec<_> = ty.vendor_prefixes.iter().map(|i| i.to_string()).collect();
        vendor_prefixes.insert(ty.rust_name.to_string(), prefixes.clone());
        vendor_prefixes.insert(ty.js_name.clone(), prefixes);
    }

    for import in &program.imports {
        let module_ident = import
            .module
            .as_ref()
            .and_then(|module| module_bindings.get(&ModuleKey::from_module(module)));
        let prefix = if module_ident.is_some() {
            "{__wry_module}."
        } else {
            ""
        };
        match &import.kind {
            ast::ImportKind::Type(ty) => tokens.extend(generate_type(
                ty,
                import.js_namespace.as_deref(),
                import.reexport.as_ref(),
                &krate,
                module_ident,
                prefix,
            )?),
            ast::ImportKind::Function(function) => tokens.extend(generate_function(
                function,
                import.js_namespace.as_deref(),
                import.reexport.as_ref(),
                &type_names,
                &type_generics,
                &vendor_prefixes,
                &krate,
                &js_sys,
                &futures,
                module_ident,
                prefix,
            )?),
            ast::ImportKind::Static(st) => tokens.extend(generate_static(
                st,
                import.js_namespace.as_deref(),
                import.reexport.as_ref(),
                &krate,
                module_ident,
                prefix,
            )?),
            ast::ImportKind::String(st) => {
                tokens.extend(generate_static_string(
                    st,
                    import.reexport.as_ref(),
                    &krate,
                )?);
            }
            ast::ImportKind::Enum(string_enum) => {
                tokens.extend(generate_string_enum(string_enum, &krate)?);
            }
            ast::ImportKind::DynamicUnion(dynamic_union) => {
                tokens.extend(generate_dynamic_union(dynamic_union, &krate)?);
            }
        }
    }

    for numeric_enum in &program.enums {
        tokens.extend(generate_numeric_enum(numeric_enum, &krate)?);
    }
    for export_struct in &program.structs {
        tokens.extend(generate_export_struct(export_struct, &krate)?);
    }
    for export in &program.exports {
        if export.rust_class.is_some() {
            tokens.extend(generate_export_method(export, &krate, &js_sys, &futures)?);
        } else {
            tokens.extend(generate_export_function(export, &krate, &js_sys, &futures)?);
        }
    }
    if let Some(main) = &output.main {
        tokens.extend(generate_main_function(main, &krate)?);
    }

    Ok(tokens)
}
