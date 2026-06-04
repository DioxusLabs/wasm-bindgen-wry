//! Code generation for wasm_bindgen macro
//!
//! This module generates Rust code that uses the wry-bindgen runtime
//! and inventory-based function registration.

mod common;
mod erasure;
mod exports;
mod imports;
mod js;
mod statics;
mod string_enum;
mod types;

use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, RandomState},
};

use crate::ast::Program;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote_spanned};

use exports::{generate_export_function, generate_export_method, generate_export_struct};
use imports::generate_function;
use statics::generate_static;
use string_enum::generate_string_enum;
use types::generate_type;

pub(crate) fn generate(program: &Program) -> syn::Result<TokenStream> {
    let mut tokens = TokenStream::new();
    let krate = &program.attrs.crate_path_tokens();

    // First generate the module for inline_js or module attribute if needed
    let mut prefix = String::new();
    let mut module_ident = None;

    // Determine the module content expression: either inline_js or include_str!(module_path)
    let module_content: Option<(proc_macro2::Span, TokenStream)> = if let Some((
        span,
        inline_js_module,
    )) = &program.attrs.inline_js
    {
        Some((*span, inline_js_module.to_token_stream()))
    } else if let Some((span, module_path)) = &program.attrs.module {
        // If path starts with '/', make it relative to CARGO_MANIFEST_DIR
        let include_expr = if module_path.starts_with('/') {
            quote_spanned! {*span=> include_str!(concat!(env!("CARGO_MANIFEST_DIR"), #module_path)) }
        } else {
            quote_spanned! {*span=> include_str!(#module_path) }
        };
        Some((*span, include_expr))
    } else {
        None
    };

    if let Some((span, content_expr)) = module_content {
        let unique_hash = {
            let s = RandomState::new();

            s.hash_one(content_expr.to_string())
        };
        let unique_ident = format_ident!("__WRY_BINDGEN_JS_MODULE_{}", unique_hash);
        // Create a shared module spec for all generated functions in this extern block.
        tokens.extend(quote_spanned! {span=>
            static #unique_ident: #krate::__rt::JsModuleSpec = #krate::__rt::JsModuleSpec::new(
                #content_expr
            );
        });
        prefix = "{__wry_module}.".to_string();
        module_ident = Some(unique_ident);
    }

    // Collect type names being defined in this block
    let type_names: HashSet<String> = program
        .types
        .iter()
        .map(|t| t.rust_name.to_string())
        .collect();
    let type_generics: HashMap<String, syn::Generics> = program
        .types
        .iter()
        .map(|t| (t.rust_name.to_string(), t.generics.clone()))
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
        let type_module = if module_ident.is_some() {
            module_ident.as_ref()
        } else {
            None
        };
        let type_prefix = if module_ident.is_some() { &prefix } else { "" };
        tokens.extend(generate_type(ty, krate, type_module, type_prefix)?);
    }

    // Generate function definitions
    for func in &program.functions {
        tokens.extend(generate_function(
            func,
            &type_names,
            &type_generics,
            &vendor_prefixes,
            krate,
            module_ident.as_ref(),
            &prefix,
        )?);
    }

    // Generate static definitions
    for st in &program.statics {
        tokens.extend(generate_static(st, krate, module_ident.as_ref(), &prefix)?);
    }

    // Generate string enum definitions
    for string_enum in &program.string_enums {
        tokens.extend(generate_string_enum(string_enum, krate)?);
    }

    // Generate exported struct definitions
    for export_struct in &program.structs {
        tokens.extend(generate_export_struct(export_struct, krate)?);
    }

    // Generate exported free function definitions
    for export_function in &program.free_functions {
        tokens.extend(generate_export_function(export_function, krate)?);
    }

    // Generate exported method definitions
    for export_method in &program.exports {
        tokens.extend(generate_export_method(export_method, krate)?);
    }

    Ok(tokens)
}
