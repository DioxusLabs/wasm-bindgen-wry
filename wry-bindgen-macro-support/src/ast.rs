//! AST definitions for wasm_bindgen macro
//!
//! This module defines the intermediate representation for parsed wasm_bindgen items.

use crate::parser::BindgenAttrs;
use quote::quote_spanned;
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
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                    {
                        return extract_simple_type_name(inner_ty);
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
    /// Attributes
    pub attrs: BindgenAttrs,
    /// Imported types
    pub types: Vec<ImportType>,
    /// Imported functions
    pub functions: Vec<ImportFunction>,
    /// Imported statics (global values)
    pub statics: Vec<ImportStatic>,
    /// String enums
    pub string_enums: Vec<StringEnum>,
    /// Exported structs
    pub structs: Vec<ExportStruct>,
    /// Exported methods from impl blocks
    pub exports: Vec<ExportMethod>,
}

/// A string enum - an enum where each variant has a string discriminant
#[derive(Debug)]
pub struct StringEnum {
    /// Visibility of the enum
    pub vis: Visibility,
    /// Rust enum name
    pub name: Ident,
    /// Variant identifiers
    pub variants: Vec<Ident>,
    /// String values for each variant (in same order as variants)
    pub variant_values: Vec<String>,
    /// User-provided attributes (like #[derive(Debug, Clone, Copy, PartialEq, Eq)])
    pub rust_attrs: Vec<syn::Attribute>,
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
    /// Vendor prefixes for fallback (e.g., webkit, moz)
    pub vendor_prefixes: Vec<Ident>,
    /// Custom is_type_of expression for type checking
    pub is_type_of: Option<syn::Expr>,
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
    /// Whether this is an async function
    pub is_async: bool,
    /// User-provided attributes (like #[cfg(...)] and #[doc = "..."])
    pub rust_attrs: Vec<syn::Attribute>,
}

impl ImportFunction {
    /// Get the function rust attributes
    pub fn fn_rust_attrs(&self) -> proc_macro2::TokenStream {
        let rust_attrs = &self.rust_attrs;
        let span = self.rust_name.span();
        quote_spanned! {span=> #(#rust_attrs)* #[allow(non_snake_case)] }
    }
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
    /// Indexing getter (e.g., `obj[index]`)
    IndexingGetter {
        /// The receiver type
        receiver: Type,
    },
    /// Indexing setter (e.g., `obj[index] = value`)
    IndexingSetter {
        /// The receiver type
        receiver: Type,
    },
    /// Indexing deleter (e.g., `delete obj[index]`)
    IndexingDeleter {
        /// The receiver type
        receiver: Type,
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

// ============================================================================
// Export Types (for Rust structs/impl blocks exposed to JavaScript)
// ============================================================================

/// An exported Rust struct
#[derive(Debug)]
pub struct ExportStruct {
    /// Visibility of the struct
    pub vis: Visibility,
    /// Rust struct name
    pub rust_name: Ident,
    /// JavaScript class name (may differ from rust_name)
    pub js_name: String,
    /// Struct fields that should be exposed
    pub fields: Vec<StructField>,
    /// Whether to generate toJSON/toString methods
    pub is_inspectable: bool,
    /// User-provided attributes (like #[derive(...)])
    pub rust_attrs: Vec<syn::Attribute>,
}

/// A field in an exported struct
#[derive(Debug)]
pub struct StructField {
    /// Visibility of the field
    pub vis: Visibility,
    /// Rust field name
    pub rust_name: Ident,
    /// JavaScript property name (may differ from rust_name)
    pub js_name: String,
    /// Field type
    pub ty: Type,
    /// Whether this field is read-only (no setter)
    pub readonly: bool,
    /// Whether to clone the value in the getter (for non-Copy types)
    pub getter_with_clone: bool,
    /// Whether to skip this field entirely
    pub skip: bool,
}

/// An exported method from an impl block
#[derive(Debug)]
pub struct ExportMethod {
    /// The struct this method belongs to
    pub class: Ident,
    /// Rust method name
    pub rust_name: Ident,
    /// JavaScript method name (may differ from rust_name)
    pub js_name: String,
    /// Kind of method
    pub kind: ExportMethodKind,
    /// Method arguments (excluding self)
    pub arguments: Vec<FunctionArg>,
    /// Return type
    pub ret: Option<Type>,
    /// Whether to wrap in try-catch
    pub catch: bool,
    /// User-provided attributes (like #[cfg(...)] and #[doc = "..."])
    pub rust_attrs: Vec<syn::Attribute>,
    /// Method visibility
    pub vis: syn::Visibility,
    /// The original method body
    pub body: syn::Block,
}

impl ExportMethod {
    /// Get the function rust attributes
    pub fn fn_rust_attrs(&self) -> proc_macro2::TokenStream {
        let rust_attrs = &self.rust_attrs;
        let span = self.rust_name.span();
        quote_spanned! {span=> #(#rust_attrs)* #[allow(non_snake_case)] }
    }
}

/// Kind of exported method
#[derive(Debug, Clone)]
pub enum ExportMethodKind {
    /// Constructor (creates new instance)
    Constructor,
    /// Instance method with a receiver
    Method {
        /// How self is passed
        self_ty: SelfType,
    },
    /// Static method (no self)
    StaticMethod,
    /// Property getter
    Getter {
        /// Property name
        property: String,
    },
    /// Property setter
    Setter {
        /// Property name
        property: String,
    },
}

/// How self is passed to a method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfType {
    /// &self - shared reference
    RefShared,
    /// &mut self - mutable reference
    RefMutable,
    /// self - by value (consumes)
    ByValue,
}

/// Parse a syn::Item into our AST
pub fn parse_item(program: &mut Program, item: syn::Item) -> syn::Result<()> {
    match item {
        syn::Item::ForeignMod(foreign) => {
            parse_foreign_mod(program, foreign)?;
        }
        syn::Item::Enum(e) => {
            let string_enum = parse_string_enum(e)?;
            program.string_enums.push(string_enum);
        }
        syn::Item::Struct(s) => {
            let export_struct = parse_struct(s, &program.attrs)?;
            program.structs.push(export_struct);
        }
        syn::Item::Impl(i) => {
            let exports = parse_impl_block(i, &program.attrs)?;
            program.exports.extend(exports);
        }
        _ => {
            return Err(syn::Error::new_spanned(
                item,
                "wasm_bindgen attribute must be on extern \"C\" block, enum, struct, or impl block",
            ));
        }
    }
    Ok(())
}

/// Parse a struct definition for export
fn parse_struct(s: syn::ItemStruct, attrs: &BindgenAttrs) -> syn::Result<ExportStruct> {
    let rust_name = s.ident.clone();
    let js_name = attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    let is_inspectable = attrs.inspectable.is_some();

    // Parse fields
    let mut fields = Vec::new();
    match &s.fields {
        syn::Fields::Named(named) => {
            for field in &named.named {
                let field_attrs = extract_wasm_bindgen_attrs(&field.attrs)?;

                // Skip fields marked with #[wasm_bindgen(skip)]
                if field_attrs.skip.is_some() {
                    continue;
                }

                let field_name = field
                    .ident
                    .clone()
                    .ok_or_else(|| syn::Error::new_spanned(field, "struct fields must be named"))?;

                let js_field_name = field_attrs
                    .js_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| field_name.to_string());

                fields.push(StructField {
                    vis: field.vis.clone(),
                    rust_name: field_name,
                    js_name: js_field_name,
                    ty: field.ty.clone(),
                    readonly: field_attrs.readonly.is_some(),
                    getter_with_clone: field_attrs.getter_with_clone.is_some(),
                    skip: false,
                });
            }
        }
        syn::Fields::Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                &s.fields,
                "tuple structs are not supported; use named fields",
            ));
        }
        syn::Fields::Unit => {
            // Unit struct - no fields, that's fine
        }
    }

    // Extract non-wasm_bindgen attributes
    let rust_attrs: Vec<syn::Attribute> = s
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("wasm_bindgen"))
        .cloned()
        .collect();

    Ok(ExportStruct {
        vis: s.vis,
        rust_name,
        js_name,
        fields,
        is_inspectable,
        rust_attrs,
    })
}

/// Parse an impl block for export
fn parse_impl_block(i: syn::ItemImpl, attrs: &BindgenAttrs) -> syn::Result<Vec<ExportMethod>> {
    // Validate: no generics, no trait impls
    if !i.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &i.generics,
            "generic impl blocks are not supported",
        ));
    }
    if i.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            &i,
            "trait impls are not supported; only inherent impls",
        ));
    }
    if i.unsafety.is_some() {
        return Err(syn::Error::new_spanned(
            i.unsafety,
            "unsafe impl blocks are not supported",
        ));
    }

    // Extract the class name from Self type
    let class = match &*i.self_ty {
        syn::Type::Path(p) => p.path.get_ident().cloned().ok_or_else(|| {
            syn::Error::new_spanned(&i.self_ty, "expected simple type name for impl block")
        })?,
        _ => {
            return Err(syn::Error::new_spanned(
                &i.self_ty,
                "expected simple type name for impl block",
            ));
        }
    };

    let mut exports = Vec::new();

    for item in &i.items {
        match item {
            syn::ImplItem::Fn(method) => {
                // Skip non-public methods
                if !matches!(method.vis, syn::Visibility::Public(_)) {
                    continue;
                }

                let method_attrs = extract_wasm_bindgen_attrs(&method.attrs)?;
                let export = parse_impl_method(&class, method, method_attrs, attrs)?;
                exports.push(export);
            }
            syn::ImplItem::Const(_) => {
                // Skip constants
            }
            syn::ImplItem::Type(_) => {
                // Skip type aliases
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    item,
                    "only methods are supported in impl blocks",
                ));
            }
        }
    }

    Ok(exports)
}

/// Parse a method in an impl block
fn parse_impl_method(
    class: &Ident,
    method: &syn::ImplItemFn,
    method_attrs: BindgenAttrs,
    _block_attrs: &BindgenAttrs,
) -> syn::Result<ExportMethod> {
    let rust_name = method.sig.ident.clone();
    let js_name = method_attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    // Determine the kind based on receiver and attributes
    let mut arguments = Vec::new();
    let mut self_ty = None;

    for (i, arg) in method.sig.inputs.iter().enumerate() {
        match arg {
            FnArg::Receiver(r) => {
                if i != 0 {
                    return Err(syn::Error::new_spanned(r, "self must be first argument"));
                }
                self_ty = Some(if r.reference.is_some() {
                    if r.mutability.is_some() {
                        SelfType::RefMutable
                    } else {
                        SelfType::RefShared
                    }
                } else {
                    SelfType::ByValue
                });
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
                arguments.push(FunctionArg {
                    name,
                    ty: (*pat_type.ty).clone(),
                });
            }
        }
    }

    // Determine method kind
    let kind = if method_attrs.is_constructor() {
        ExportMethodKind::Constructor
    } else if let Some((_, name)) = &method_attrs.getter {
        let property = name.clone().unwrap_or_else(|| js_name.clone());
        ExportMethodKind::Getter { property }
    } else if let Some((_, name)) = &method_attrs.setter {
        let property = name
            .clone()
            .unwrap_or_else(|| js_name.strip_prefix("set_").unwrap_or(&js_name).to_string());
        ExportMethodKind::Setter { property }
    } else if let Some(st) = self_ty {
        ExportMethodKind::Method { self_ty: st }
    } else {
        ExportMethodKind::StaticMethod
    };

    // Parse return type
    let ret = match &method.sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => Some((**ty).clone()),
    };

    // Extract non-wasm_bindgen attributes
    let rust_attrs: Vec<syn::Attribute> = method
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("wasm_bindgen"))
        .cloned()
        .collect();

    Ok(ExportMethod {
        class: class.clone(),
        rust_name,
        js_name,
        kind,
        arguments,
        ret,
        catch: method_attrs.catch.is_some(),
        rust_attrs,
        vis: method.vis.clone(),
        body: method.block.clone(),
    })
}

/// Parse an extern "C" block
fn parse_foreign_mod(program: &mut Program, foreign: syn::ItemForeignMod) -> syn::Result<()> {
    for item in foreign.items {
        match item {
            syn::ForeignItem::Fn(f) => {
                // Parse per-function attributes from #[wasm_bindgen(...)] on the function
                let fn_attrs = extract_wasm_bindgen_attrs(&f.attrs)?;
                let func = parse_foreign_fn(f, fn_attrs)?;
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
            if let Some(span) = parsed.indexing_getter {
                combined.indexing_getter = Some(span);
            }
            if let Some(span) = parsed.indexing_setter {
                combined.indexing_setter = Some(span);
            }
            if let Some(span) = parsed.indexing_deleter {
                combined.indexing_deleter = Some(span);
            }
            if let Some(v) = parsed.is_type_of {
                combined.is_type_of = Some(v);
            }
            combined.vendor_prefixes.extend(parsed.vendor_prefixes);
        }
    }

    Ok(combined)
}

/// Parse a foreign function declaration
fn parse_foreign_fn(f: syn::ForeignItemFn, attrs: BindgenAttrs) -> syn::Result<ImportFunction> {
    let is_async = f.sig.asyncness.is_some();
    let rust_name = f.sig.ident.clone();
    let js_name = attrs
        .js_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| rust_name.to_string());

    let js_class = attrs.js_class().map(|s| s.to_string());
    let js_namespace = attrs.js_namespace.as_ref().map(|(_, v)| v.clone());

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
    } else if attrs.is_indexing_getter() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "indexing_getter must have a receiver argument")
        })?;
        ImportFunctionKind::IndexingGetter { receiver }
    } else if attrs.is_indexing_setter() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "indexing_setter must have a receiver argument")
        })?;
        ImportFunctionKind::IndexingSetter { receiver }
    } else if attrs.is_indexing_deleter() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "indexing_deleter must have a receiver argument")
        })?;
        ImportFunctionKind::IndexingDeleter { receiver }
    } else if attrs.is_method() {
        let receiver = receiver.ok_or_else(|| {
            syn::Error::new_spanned(&f.sig, "method must have a receiver argument")
        })?;
        ImportFunctionKind::Method { receiver }
    } else {
        ImportFunctionKind::Normal
    };

    // Extract non-wasm_bindgen attributes to preserve (like #[cfg(...)] and #[doc = "..."])
    let rust_attrs: Vec<syn::Attribute> = f
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("wasm_bindgen"))
        .cloned()
        .collect();

    Ok(ImportFunction {
        vis: f.vis,
        rust_name,
        js_name,
        js_class,
        js_namespace,
        arguments,
        ret,
        kind,
        catch: attrs.catch.is_some(),
        structural: attrs.is_structural(),
        variadic: attrs.variadic.is_some(),
        is_async,
        rust_attrs,
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
    let vendor_prefixes: Vec<Ident> = attrs.vendor_prefixes.into_iter().map(|(_, i)| i).collect();
    let is_type_of = attrs.is_type_of.map(|(_, e)| e);

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
        vendor_prefixes,
        is_type_of,
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

/// Parse a string enum - an enum where variants have string discriminants like:
/// ```ignore
/// enum Color {
///     Red = "red",
///     Green = "green",
/// }
/// ```
fn parse_string_enum(e: syn::ItemEnum) -> syn::Result<StringEnum> {
    let mut variants = Vec::new();
    let mut variant_values = Vec::new();

    for variant in &e.variants {
        // Check that the variant has no fields (unit variant)
        if !matches!(variant.fields, syn::Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "wasm_bindgen string enums only support unit variants",
            ));
        }

        // Extract the string discriminant
        let discriminant = variant.discriminant.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                variant,
                "wasm_bindgen string enum variants must have string discriminants (e.g., Variant = \"value\")",
            )
        })?;

        // The discriminant must be a string literal
        let string_value = match &discriminant.1 {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => s.value(),
            _ => {
                return Err(syn::Error::new_spanned(
                    &discriminant.1,
                    "wasm_bindgen string enum discriminants must be string literals",
                ));
            }
        };

        variants.push(variant.ident.clone());
        variant_values.push(string_value);
    }

    // Extract non-wasm_bindgen attributes to preserve (like #[derive(...)])
    let rust_attrs: Vec<syn::Attribute> = e
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("wasm_bindgen"))
        .cloned()
        .collect();

    Ok(StringEnum {
        vis: e.vis,
        name: e.ident,
        variants,
        variant_values,
        rust_attrs,
    })
}
