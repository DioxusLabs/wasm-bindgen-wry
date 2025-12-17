//! AST definitions for wasm_bindgen macro
//!
//! This module defines the intermediate representation for parsed wasm_bindgen items.

use crate::parser::BindgenAttrs;
use syn::{FnArg, Ident, Pat, Path, ReturnType, Type, Visibility};

/// Extract a simple type name from a Type
/// Handles simple types like `Foo` and generic types like `Result<Foo, Bar>`
fn extract_simple_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => {
            // Check for simple ident first
            if let Some(ident) = p.path.get_ident() {
                return Some(ident.to_string());
            }
            // Check for generic types like Result<Foo, Bar> - extract the first type arg
            if let Some(segment) = p.path.segments.last() {
                let seg_name = segment.ident.to_string();
                if seg_name == "Result" || seg_name == "Option" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            return extract_simple_type_name(inner_ty);
                        }
                    }
                } else {
                    // For other types, return the segment name
                    return Some(seg_name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Top-level program containing all parsed items
#[derive(Debug, Default)]
pub struct Program {
    /// Imported types
    pub types: Vec<ImportType>,
    /// Imported functions
    pub functions: Vec<ImportFunction>,
    /// Imported statics (global values)
    pub statics: Vec<ImportStatic>,
}

/// An imported JavaScript type
#[derive(Debug)]
pub struct ImportType {
    /// Visibility of the type
    pub vis: Visibility,
    /// Rust type name
    pub rust_name: Ident,
    /// JavaScript name (may differ from rust_name)
    pub js_name: String,
    /// Parent types (from `extends` attributes)
    pub extends: Vec<Path>,
    /// TypeScript type override
    pub typescript_type: Option<String>,
    /// User-provided derive attributes (e.g., Clone, Debug)
    pub derives: Vec<syn::Attribute>,
}

/// An imported JavaScript function
#[derive(Debug)]
pub struct ImportFunction {
    /// Visibility of the function
    pub vis: Visibility,
    /// Rust function name
    pub rust_name: Ident,
    /// JavaScript name (may differ from rust_name)
    pub js_name: String,
    /// The class this method belongs to (if any)
    pub js_class: Option<String>,
    /// JavaScript namespace
    pub js_namespace: Option<Vec<String>>,
    /// Inline JavaScript code (from block-level inline_js attribute)
    pub inline_js: Option<String>,
    /// Function arguments (excluding self for methods)
    pub arguments: Vec<FunctionArg>,
    /// Return type
    pub ret: Option<Type>,
    /// Kind of function
    pub kind: ImportFunctionKind,
    /// Whether to catch JS exceptions
    pub catch: bool,
    /// Whether this uses structural typing
    pub structural: bool,
    /// Whether this is variadic
    pub variadic: bool,
}

/// Argument to an imported function
#[derive(Debug)]
pub struct FunctionArg {
    /// Argument name
    pub name: Ident,
    /// Argument type
    pub ty: Type,
}

/// Kind of imported function
#[derive(Debug)]
pub enum ImportFunctionKind {
    /// Regular function (not a method)
    Normal,
    /// Instance method
    Method {
        /// The receiver type
        receiver: Type,
    },
    /// Property getter
    Getter {
        /// The receiver type
        receiver: Type,
        /// Property name (may differ from function name)
        property: String,
    },
    /// Property setter
    Setter {
        /// The receiver type
        receiver: Type,
        /// Property name (may differ from function name)
        property: String,
    },
    /// Constructor
    Constructor {
        /// The class being constructed
        class: String,
    },
    /// Static method
    StaticMethod {
        /// The class the method belongs to
        class: String,
    },
}

/// An imported JavaScript static value (global)
#[derive(Debug)]
pub struct ImportStatic {
    /// Visibility of the static
    pub vis: Visibility,
    /// Rust name for the static
    pub rust_name: Ident,
    /// JavaScript name (may differ from rust_name)
    pub js_name: String,
    /// The type of the static value
    pub ty: Type,
    /// JavaScript namespace (if any)
    pub js_namespace: Option<Vec<String>>,
    /// Whether this uses thread_local_v2 lazy initialization
    pub thread_local_v2: bool,
}

/// Parse a syn::Item into our AST
pub fn parse_item(program: &mut Program, item: syn::Item, attrs: BindgenAttrs) -> syn::Result<()> {
    match item {
        syn::Item::ForeignMod(foreign) => {
            parse_foreign_mod(program, foreign, attrs)?;
        }
        syn::Item::Struct(s) => {
            // Structs with wasm_bindgen become exported types
            // For now, we only support imported types via extern "C"
            return Err(syn::Error::new_spanned(
                s,
                "wasm_bindgen on structs is not yet supported; use extern \"C\" blocks",
            ));
        }
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "wasm_bindgen attribute must be on extern \"C\" block",
            ));
        }
    }
    Ok(())
}

/// Parse an extern "C" block
fn parse_foreign_mod(
    program: &mut Program,
    foreign: syn::ItemForeignMod,
    block_attrs: BindgenAttrs,
) -> syn::Result<()> {
    // Extract block-level inline_js if present
    let block_inline_js = block_attrs.inline_js.map(|(_, js)| js);

    for item in foreign.items {
        match item {
            syn::ForeignItem::Fn(f) => {
                // Parse per-function attributes from #[wasm_bindgen(...)] on the function
                let fn_attrs = extract_wasm_bindgen_attrs(&f.attrs)?;
                let func = parse_foreign_fn(f, fn_attrs, block_inline_js.clone())?;
                program.functions.push(func);
            }
            syn::ForeignItem::Type(t) => {
                // Parse per-type attributes
                let type_attrs = extract_wasm_bindgen_attrs(&t.attrs)?;
                let ty = parse_foreign_type(t, type_attrs)?;
                program.types.push(ty);
            }
            syn::ForeignItem::Static(s) => {
                // Parse per-static attributes
                let static_attrs = extract_wasm_bindgen_attrs(&s.attrs)?;
                let st = parse_foreign_static(s, static_attrs)?;
                program.statics.push(st);
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "only functions, types, and statics are supported in extern blocks",
                ));
            }
        }
    }
    Ok(())
}

/// Extract wasm_bindgen attributes from an attribute list
fn extract_wasm_bindgen_attrs(attrs: &[syn::Attribute]) -> syn::Result<BindgenAttrs> {
    let mut combined = BindgenAttrs::default();

    for attr in attrs {
        if attr.path().is_ident("wasm_bindgen") {
            // Handle both #[wasm_bindgen] and #[wasm_bindgen(...)]
            let tokens = match &attr.meta {
                syn::Meta::Path(_) => proc_macro2::TokenStream::new(), // Empty - no parentheses
                syn::Meta::List(list) => list.tokens.clone(),
                syn::Meta::NameValue(_) => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "wasm_bindgen does not support = syntax at top level",
                    ));
                }
            };
            let parsed = crate::parser::parse_attrs(tokens)?;

            // Merge attributes
            if let Some(span) = parsed.method {
                combined.method = Some(span);
            }
            if let Some(span) = parsed.structural {
                combined.structural = Some(span);
            }
            if let Some(v) = parsed.js_name {
                combined.js_name = Some(v);
            }
            if let Some(v) = parsed.js_class {
                combined.js_class = Some(v);
            }
            if let Some(v) = parsed.js_namespace {
                combined.js_namespace = Some(v);
            }
            if let Some(v) = parsed.getter {
                combined.getter = Some(v);
            }
            if let Some(v) = parsed.setter {
                combined.setter = Some(v);
            }
            if let Some(span) = parsed.constructor {
                combined.constructor = Some(span);
            }
            if let Some(span) = parsed.catch {
                combined.catch = Some(span);
            }
            combined.extends.extend(parsed.extends);
            if let Some(v) = parsed.static_method_of {
                combined.static_method_of = Some(v);
            }
            if let Some(span) = parsed.variadic {
                combined.variadic = Some(span);
            }
            if let Some(v) = parsed.typescript_type {
                combined.typescript_type = Some(v);
            }
            if let Some(span) = parsed.thread_local_v2 {
                combined.thread_local_v2 = Some(span);
            }
        }
    }

    Ok(combined)
}

/// Parse a foreign function declaration
fn parse_foreign_fn(
    f: syn::ForeignItemFn,
    attrs: BindgenAttrs,
    block_inline_js: Option<String>,
) -> syn::Result<ImportFunction> {
    let rust_name = f.sig.ident.clone();
    let js_name = attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    let js_class = attrs.js_class().map(|s| s.to_string());
    let js_namespace = attrs.js_namespace.as_ref().map(|(_, v)| v.clone());
    let inline_js = block_inline_js;

    // Parse arguments
    let mut arguments = Vec::new();
    let mut receiver = None;
    let mut first_arg = true;

    for arg in &f.sig.inputs {
        match arg {
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    arg,
                    "self receivers are not supported in extern blocks",
                ));
            }
            FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.clone(),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            pat_type,
                            "complex patterns not supported",
                        ));
                    }
                };
                let ty = (*pat_type.ty).clone();

                // Check if this is the receiver for a method
                if first_arg && (attrs.is_method() || attrs.is_getter() || attrs.is_setter()) {
                    receiver = Some(ty);
                } else {
                    arguments.push(FunctionArg { name, ty });
                }
                first_arg = false;
            }
        }
    }

    // Parse return type
    let ret = match &f.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some((**ty).clone()),
    };

    // Determine function kind
    let kind = if attrs.is_constructor() {
        // For constructors, ALWAYS use return type for the Rust impl block
        // js_class is for JavaScript, not for Rust type name
        let class = if let Some(ref ret_ty) = ret {
            if let Some(name) = extract_simple_type_name(ret_ty) {
                name
            } else {
                // Fallback to js_class or js_name if return type isn't simple
                js_class.clone().unwrap_or_else(|| js_name.clone())
            }
        } else {
            js_class.clone().unwrap_or_else(|| js_name.clone())
        };
        ImportFunctionKind::Constructor { class }
    } else if let Some((_, ref ident)) = attrs.static_method_of {
        ImportFunctionKind::StaticMethod {
            class: ident.to_string(),
        }
    } else if attrs.is_getter() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "getter must have a receiver argument")
        })?;
        let property = attrs
            .getter
            .as_ref()
            .and_then(|(_, n)| n.clone())
            .unwrap_or_else(|| js_name.clone());
        ImportFunctionKind::Getter { receiver, property }
    } else if attrs.is_setter() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "setter must have a receiver argument")
        })?;
        let property = attrs
            .setter
            .as_ref()
            .and_then(|(_, n)| n.clone())
            .unwrap_or_else(|| {
                // Remove "set_" prefix if present
                js_name.strip_prefix("set_").unwrap_or(&js_name).to_string()
            });
        ImportFunctionKind::Setter { receiver, property }
    } else if attrs.is_method() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "method must have a receiver argument")
        })?;
        ImportFunctionKind::Method { receiver }
    } else {
        ImportFunctionKind::Normal
    };

    Ok(ImportFunction {
        vis: f.vis,
        rust_name,
        js_name,
        js_class,
        js_namespace,
        inline_js,
        arguments,
        ret,
        kind,
        catch: attrs.catch.is_some(),
        structural: attrs.is_structural(),
        variadic: attrs.variadic.is_some(),
    })
}

/// Parse a foreign type declaration
fn parse_foreign_type(t: syn::ForeignItemType, attrs: BindgenAttrs) -> syn::Result<ImportType> {
    let rust_name = t.ident.clone();
    let js_name = attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    let extends: Vec<Path> = attrs.extends.into_iter().map(|(_, p)| p).collect();
    let typescript_type = attrs.typescript_type.map(|(_, t)| t);

    // Extract derive attributes (non-wasm_bindgen attributes that should be preserved)
    let derives: Vec<syn::Attribute> = t
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("wasm_bindgen"))
        .cloned()
        .collect();

    Ok(ImportType {
        vis: t.vis,
        rust_name,
        js_name,
        extends,
        typescript_type,
        derives,
    })
}

/// Parse a foreign static declaration
fn parse_foreign_static(
    s: syn::ForeignItemStatic,
    attrs: BindgenAttrs,
) -> syn::Result<ImportStatic> {
    // Mutable statics are not supported
    if let syn::StaticMutability::Mut(_) = s.mutability {
        return Err(syn::Error::new_spanned(
            s.mutability,
            "cannot import mutable statics",
        ));
    }

    let rust_name = s.ident.clone();
    let js_name = attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    let js_namespace = attrs.js_namespace.as_ref().map(|(_, v)| v.clone());
    let thread_local_v2 = attrs.is_thread_local_v2();

    Ok(ImportStatic {
        vis: s.vis,
        rust_name,
        js_name,
        ty: *s.ty,
        js_namespace,
        thread_local_v2,
    })
}
