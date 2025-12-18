//! Parser for wasm_bindgen attributes
//!
//! This module parses the attributes on `#[wasm_bindgen(...)]` into a structured form.

use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, LitStr, Path, Token};

/// Parse an identifier or keyword as a string.
/// This allows using Rust keywords like `return`, `self`, `type`, etc. as JS names.
fn parse_any_ident(input: ParseStream) -> syn::Result<String> {
    // Try to parse as a regular identifier first
    if input.peek(Ident::peek_any) {
        let ident = input.call(Ident::parse_any)?;
        return Ok(ident.to_string());
    }

    Err(input.error("expected identifier"))
}

/// Parsed wasm_bindgen attributes
#[derive(Debug, Default)]
pub struct BindgenAttrs {
    /// The `method` attribute - marks a function as an instance method
    pub method: Option<Span>,
    /// The `structural` attribute - use structural/duck typing
    pub structural: Option<Span>,
    /// The `js_name` attribute - JavaScript name override
    pub js_name: Option<(Span, String)>,
    /// The `js_class` attribute - JavaScript class name
    pub js_class: Option<(Span, String)>,
    /// The `js_namespace` attribute - JavaScript namespace
    pub js_namespace: Option<(Span, Vec<String>)>,
    /// The `getter` attribute - property getter
    pub getter: Option<(Span, Option<String>)>,
    /// The `setter` attribute - property setter
    pub setter: Option<(Span, Option<String>)>,
    /// The `constructor` attribute - class constructor
    pub constructor: Option<Span>,
    /// The `catch` attribute - wraps in try-catch
    pub catch: Option<Span>,
    /// The `extends` attribute - type inheritance
    pub extends: Vec<(Span, Path)>,
    /// The `static_method_of` attribute - static method
    pub static_method_of: Option<(Span, Ident)>,
    /// The `variadic` attribute - variable arguments
    pub variadic: Option<Span>,
    /// The `typescript_type` attribute - TypeScript type override
    pub typescript_type: Option<(Span, String)>,
    /// The `inline_js` attribute - inline JavaScript code (accepts any expression)
    pub inline_js: Option<(Span, Expr)>,
    /// The `thread_local_v2` attribute - marks a static as lazily initialized
    pub thread_local_v2: Option<Span>,
    /// The `is_type_of` attribute - custom type checking expression
    pub is_type_of: Option<(Span, Expr)>,
    /// The `indexing_getter` attribute - array-like indexing getter
    pub indexing_getter: Option<Span>,
    /// The `indexing_setter` attribute - array-like indexing setter
    pub indexing_setter: Option<Span>,
    /// The `indexing_deleter` attribute - array-like indexing deleter
    pub indexing_deleter: Option<Span>,
    /// The `final` attribute - mark type as final
    pub final_: Option<Span>,
    /// The `readonly` attribute - mark property as read-only
    pub readonly: Option<Span>,
    /// The `crate` attribute - custom crate path for imports (default: wasm_bindgen)
    pub crate_path: Option<(Span, Path)>,
}

impl BindgenAttrs {
    /// Check if this is a method (has `method` or `structural` attribute)
    pub fn is_method(&self) -> bool {
        self.method.is_some()
    }

    /// Check if this is a getter
    pub fn is_getter(&self) -> bool {
        self.getter.is_some()
    }

    /// Check if this is a setter
    pub fn is_setter(&self) -> bool {
        self.setter.is_some()
    }

    /// Check if this is a constructor
    pub fn is_constructor(&self) -> bool {
        self.constructor.is_some()
    }

    /// Get the effective JS name (js_name override or None)
    pub fn js_name(&self) -> Option<&str> {
        self.js_name.as_ref().map(|(_, s)| s.as_str())
    }

    /// Get the JS class name
    pub fn js_class(&self) -> Option<&str> {
        self.js_class.as_ref().map(|(_, s)| s.as_str())
    }

    /// Check if structural typing is enabled
    pub fn is_structural(&self) -> bool {
        self.structural.is_some()
    }

    /// Check if this is a thread-local static
    pub fn is_thread_local_v2(&self) -> bool {
        self.thread_local_v2.is_some()
    }

    /// Get the crate path as a TokenStream, defaulting to `wasm_bindgen`
    pub fn crate_path_tokens(&self) -> TokenStream {
        match &self.crate_path {
            Some((_, path)) => path.to_token_stream(),
            None => {
                let ident = Ident::new("wasm_bindgen", Span::call_site());
                ident.to_token_stream()
            }
        }
    }
}

/// A single attribute within wasm_bindgen(...)
enum BindgenAttr {
    Method(Span),
    Structural(Span),
    JsName(Span, String),
    JsClass(Span, String),
    JsNamespace(Span, Vec<String>),
    Getter(Span, Option<String>),
    Setter(Span, Option<String>),
    Constructor(Span),
    Catch(Span),
    Extends(Span, Path),
    StaticMethodOf(Span, Ident),
    Variadic(Span),
    TypescriptType(Span, String),
    InlineJs(Span, Expr),
    ThreadLocalV2(Span),
    IsTypeOf(Span, Expr),
    IndexingGetter(Span),
    IndexingSetter(Span),
    IndexingDeleter(Span),
    Final(Span),
    Readonly(Span),
    Crate(Span, Path),
}

impl Parse for BindgenAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Use parse_any to handle keywords like `crate`
        let ident = input.call(Ident::parse_any)?;
        let span = ident.span();
        let name = ident.to_string();

        match name.as_str() {
            "method" => Ok(BindgenAttr::Method(span)),
            "structural" => Ok(BindgenAttr::Structural(span)),
            "constructor" => Ok(BindgenAttr::Constructor(span)),
            "catch" => Ok(BindgenAttr::Catch(span)),
            "variadic" => Ok(BindgenAttr::Variadic(span)),
            "thread_local_v2" => Ok(BindgenAttr::ThreadLocalV2(span)),

            "js_name" => {
                input.parse::<Token![=]>()?;
                let name = if input.peek(LitStr) {
                    input.parse::<LitStr>()?.value()
                } else {
                    // Support keywords like `return`, `self`, etc.
                    parse_any_ident(input)?
                };
                Ok(BindgenAttr::JsName(span, name))
            }

            "js_class" => {
                input.parse::<Token![=]>()?;
                let name = if input.peek(LitStr) {
                    input.parse::<LitStr>()?.value()
                } else {
                    input.parse::<Ident>()?.to_string()
                };
                Ok(BindgenAttr::JsClass(span, name))
            }

            "js_namespace" => {
                input.parse::<Token![=]>()?;
                let namespace = if input.peek(syn::token::Bracket) {
                    // Parse array: js_namespace = ["a", "b"]
                    let content;
                    syn::bracketed!(content in input);
                    let names: Punctuated<LitStr, Token![,]> =
                        Punctuated::parse_terminated(&content)?;
                    names.into_iter().map(|s| s.value()).collect()
                } else if input.peek(LitStr) {
                    vec![input.parse::<LitStr>()?.value()]
                } else {
                    vec![input.parse::<Ident>()?.to_string()]
                };
                Ok(BindgenAttr::JsNamespace(span, namespace))
            }

            "getter" => {
                let name = if input.peek(Token![=]) {
                    input.parse::<Token![=]>()?;
                    if input.peek(LitStr) {
                        Some(input.parse::<LitStr>()?.value())
                    } else {
                        Some(input.parse::<Ident>()?.to_string())
                    }
                } else {
                    None
                };
                Ok(BindgenAttr::Getter(span, name))
            }

            "setter" => {
                let name = if input.peek(Token![=]) {
                    input.parse::<Token![=]>()?;
                    if input.peek(LitStr) {
                        Some(input.parse::<LitStr>()?.value())
                    } else {
                        Some(input.parse::<Ident>()?.to_string())
                    }
                } else {
                    None
                };
                Ok(BindgenAttr::Setter(span, name))
            }

            "extends" => {
                input.parse::<Token![=]>()?;
                let path: Path = input.parse()?;
                Ok(BindgenAttr::Extends(span, path))
            }

            "static_method_of" => {
                input.parse::<Token![=]>()?;
                let ident: Ident = input.parse()?;
                Ok(BindgenAttr::StaticMethodOf(span, ident))
            }

            "typescript_type" => {
                input.parse::<Token![=]>()?;
                let ty = if input.peek(LitStr) {
                    input.parse::<LitStr>()?.value()
                } else {
                    // Also accept identifier (for macro expansion like $name)
                    parse_any_ident(input)?
                };
                Ok(BindgenAttr::TypescriptType(span, ty))
            }

            "inline_js" => {
                input.parse::<Token![=]>()?;
                let expr: Expr = input.parse()?;
                Ok(BindgenAttr::InlineJs(span, expr))
            }

            "is_type_of" => {
                input.parse::<Token![=]>()?;
                let expr: Expr = input.parse()?;
                Ok(BindgenAttr::IsTypeOf(span, expr))
            }

            "indexing_getter" => Ok(BindgenAttr::IndexingGetter(span)),
            "indexing_setter" => Ok(BindgenAttr::IndexingSetter(span)),
            "indexing_deleter" => Ok(BindgenAttr::IndexingDeleter(span)),
            "final" => Ok(BindgenAttr::Final(span)),
            "readonly" => Ok(BindgenAttr::Readonly(span)),

            "crate" => {
                input.parse::<Token![=]>()?;
                // Handle `crate` keyword specially since it's not a valid Path
                let path = if input.peek(Token![crate]) {
                    let crate_token: Token![crate] = input.parse()?;
                    syn::Path::from(syn::PathSegment::from(syn::Ident::new(
                        "crate",
                        crate_token.span,
                    )))
                } else {
                    input.parse()?
                };
                Ok(BindgenAttr::Crate(span, path))
            }

            _ => Err(syn::Error::new(
                span,
                format!("unknown wasm_bindgen attribute: `{}`", name),
            )),
        }
    }
}

/// Parse a comma-separated list of attributes
struct BindgenAttrList(Vec<BindgenAttr>);

impl Parse for BindgenAttrList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Handle empty input (e.g., #[wasm_bindgen] with no parentheses)
        if input.is_empty() {
            return Ok(BindgenAttrList(Vec::new()));
        }
        let attrs = Punctuated::<BindgenAttr, Token![,]>::parse_terminated(input)?;
        Ok(BindgenAttrList(attrs.into_iter().collect()))
    }
}

/// Parse the attribute token stream into BindgenAttrs
pub fn parse_attrs(attr: TokenStream) -> syn::Result<BindgenAttrs> {
    // Handle empty token stream - allows #[wasm_bindgen] without parentheses
    if attr.is_empty() {
        return Ok(BindgenAttrs::default());
    }
    let list: BindgenAttrList = syn::parse2(attr)?;
    let mut result = BindgenAttrs::default();

    for attr in list.0 {
        match attr {
            BindgenAttr::Method(span) => {
                if result.method.is_some() {
                    return Err(syn::Error::new(span, "duplicate `method` attribute"));
                }
                result.method = Some(span);
            }
            BindgenAttr::Structural(span) => {
                if result.structural.is_some() {
                    return Err(syn::Error::new(span, "duplicate `structural` attribute"));
                }
                result.structural = Some(span);
            }
            BindgenAttr::JsName(span, name) => {
                if result.js_name.is_some() {
                    return Err(syn::Error::new(span, "duplicate `js_name` attribute"));
                }
                result.js_name = Some((span, name));
            }
            BindgenAttr::JsClass(span, name) => {
                if result.js_class.is_some() {
                    return Err(syn::Error::new(span, "duplicate `js_class` attribute"));
                }
                result.js_class = Some((span, name));
            }
            BindgenAttr::JsNamespace(span, ns) => {
                if result.js_namespace.is_some() {
                    return Err(syn::Error::new(span, "duplicate `js_namespace` attribute"));
                }
                result.js_namespace = Some((span, ns));
            }
            BindgenAttr::Getter(span, name) => {
                if result.getter.is_some() {
                    return Err(syn::Error::new(span, "duplicate `getter` attribute"));
                }
                result.getter = Some((span, name));
            }
            BindgenAttr::Setter(span, name) => {
                if result.setter.is_some() {
                    return Err(syn::Error::new(span, "duplicate `setter` attribute"));
                }
                result.setter = Some((span, name));
            }
            BindgenAttr::Constructor(span) => {
                if result.constructor.is_some() {
                    return Err(syn::Error::new(span, "duplicate `constructor` attribute"));
                }
                result.constructor = Some(span);
            }
            BindgenAttr::Catch(span) => {
                if result.catch.is_some() {
                    return Err(syn::Error::new(span, "duplicate `catch` attribute"));
                }
                result.catch = Some(span);
            }
            BindgenAttr::Extends(span, path) => {
                result.extends.push((span, path));
            }
            BindgenAttr::StaticMethodOf(span, ident) => {
                if result.static_method_of.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `static_method_of` attribute",
                    ));
                }
                result.static_method_of = Some((span, ident));
            }
            BindgenAttr::Variadic(span) => {
                if result.variadic.is_some() {
                    return Err(syn::Error::new(span, "duplicate `variadic` attribute"));
                }
                result.variadic = Some(span);
            }
            BindgenAttr::TypescriptType(span, ty) => {
                if result.typescript_type.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `typescript_type` attribute",
                    ));
                }
                result.typescript_type = Some((span, ty));
            }
            BindgenAttr::InlineJs(span, js) => {
                if result.inline_js.is_some() {
                    return Err(syn::Error::new(span, "duplicate `inline_js` attribute"));
                }
                result.inline_js = Some((span, js));
            }
            BindgenAttr::ThreadLocalV2(span) => {
                if result.thread_local_v2.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `thread_local_v2` attribute",
                    ));
                }
                result.thread_local_v2 = Some(span);
            }
            BindgenAttr::IsTypeOf(span, expr) => {
                if result.is_type_of.is_some() {
                    return Err(syn::Error::new(span, "duplicate `is_type_of` attribute"));
                }
                result.is_type_of = Some((span, expr));
            }
            BindgenAttr::IndexingGetter(span) => {
                if result.indexing_getter.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `indexing_getter` attribute",
                    ));
                }
                result.indexing_getter = Some(span);
            }
            BindgenAttr::IndexingSetter(span) => {
                if result.indexing_setter.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `indexing_setter` attribute",
                    ));
                }
                result.indexing_setter = Some(span);
            }
            BindgenAttr::IndexingDeleter(span) => {
                if result.indexing_deleter.is_some() {
                    return Err(syn::Error::new(
                        span,
                        "duplicate `indexing_deleter` attribute",
                    ));
                }
                result.indexing_deleter = Some(span);
            }
            BindgenAttr::Final(span) => {
                if result.final_.is_some() {
                    return Err(syn::Error::new(span, "duplicate `final` attribute"));
                }
                result.final_ = Some(span);
            }
            BindgenAttr::Readonly(span) => {
                if result.readonly.is_some() {
                    return Err(syn::Error::new(span, "duplicate `readonly` attribute"));
                }
                result.readonly = Some(span);
            }
            BindgenAttr::Crate(span, path) => {
                if result.crate_path.is_some() {
                    return Err(syn::Error::new(span, "duplicate `crate` attribute"));
                }
                result.crate_path = Some((span, path));
            }
        }
    }

    Ok(result)
}
