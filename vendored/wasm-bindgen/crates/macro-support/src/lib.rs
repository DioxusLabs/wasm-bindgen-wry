//! This crate contains implementation APIs for the `#[wasm_bindgen]` attribute.

#![doc(html_root_url = "https://docs.rs/wasm-bindgen-macro-support/0.2")]

#[macro_use]
mod error;

pub mod ast;
#[cfg(feature = "expand")]
mod codegen;
#[cfg(feature = "expand")]
mod encode;
#[cfg(feature = "expand")]
mod generics;
mod hash;
pub mod parser;

#[cfg(feature = "expand")]
use codegen::TryToTokens;
pub use error::Diagnostic;
pub use parser::{BindgenAttr, BindgenAttrs, JsNamespace};
use parser::{ConvertToAst, MacroParse};
use proc_macro2::TokenStream;
use quote::quote;
use quote::ToTokens;
#[cfg(feature = "expand")]
use quote::TokenStreamExt;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Result as SynResult};
use syn::Token;

/// Parsed upstream macro-support AST plus Rust tokens preserved by the parser.
///
/// The parser emits these tokens for user Rust items that must remain in the
/// expanded output while the AST carries wasm-bindgen metadata for generated
/// glue.
pub struct ParseOutput {
    pub program: ast::Program,
    pub tokens: TokenStream,
    pub main: Option<syn::Ident>,
}

/// Parse a `#[wasm_bindgen]` input item into the upstream macro-support AST.
pub fn parse(attr: TokenStream, input: TokenStream) -> Result<ast::Program, Diagnostic> {
    Ok(parse_with_tokens(attr, input)?.program)
}

/// Parse a `#[wasm_bindgen]` input item into the upstream macro-support AST and
/// return the Rust tokens the upstream parser preserves.
pub fn parse_with_tokens(attr: TokenStream, input: TokenStream) -> Result<ParseOutput, Diagnostic> {
    parser::reset_attrs_used();

    let item = match syn::parse2::<syn::Item>(input)? {
        syn::Item::Struct(item) => return parse_struct_item_with_tokens(attr, item),
        syn::Item::Impl(item) => return parse_impl_item_with_tokens(attr, item),
        syn::Item::Const(item) => return parse_const_item_with_tokens(attr, item),
        item => item,
    };

    let opts: BindgenAttrs = syn::parse2(attr)?;
    let main = match &item {
        syn::Item::Fn(item) if opts.main().is_some() => Some(item.sig.ident.clone()),
        _ => None,
    };
    let mut tokens = TokenStream::new();
    let mut program = ast::Program::default();
    item.macro_parse(&mut program, (Some(opts), &mut tokens))?;
    parser::unused_attrs_diagnostic()?;

    Ok(ParseOutput {
        program,
        tokens,
        main,
    })
}

fn parse_const_item_with_tokens(
    attr: TokenStream,
    item: syn::ItemConst,
) -> Result<ParseOutput, Diagnostic> {
    let opts = syn::parse2(attr)?;
    let mut program = ast::Program::default();
    item.clone().macro_parse(&mut program, opts)?;
    parser::unused_attrs_diagnostic()?;

    Ok(ParseOutput {
        program,
        tokens: item.to_token_stream(),
        main: None,
    })
}

fn parse_struct_item_with_tokens(
    attr: TokenStream,
    mut item: syn::ItemStruct,
) -> Result<ParseOutput, Diagnostic> {
    let opts: BindgenAttrs = syn::parse2(attr.clone())?;
    let wasm_bindgen = opts
        .wasm_bindgen()
        .cloned()
        .unwrap_or_else(|| syn::parse_quote! { ::wasm_bindgen });
    let extends_path = opts.attrs.iter().find_map(|(_, a)| match a {
        parser::BindgenAttr::Extends(_, path) => Some(path.clone()),
        _ => None,
    });
    parser::inject_parent_field(&mut item, extends_path.as_ref(), &wasm_bindgen)?;

    // The upstream struct path is two-stage: the attribute macro re-emits the
    // struct with `#[wasm_bindgen(...)]`, then the derive marker parses it.
    // Reset attribute accounting for that marker-like parse.
    parser::reset_attrs_used();
    let mut item: syn::ItemStruct = syn::parse2(quote! {
        #[wasm_bindgen(#attr)]
        #item
    })?;

    let mut program = ast::Program::default();
    program.structs.push((&mut item).convert(&program)?);
    parser::unused_attrs_diagnostic()?;

    Ok(ParseOutput {
        program,
        tokens: item.to_token_stream(),
        main: None,
    })
}

fn parse_impl_item_with_tokens(
    attr: TokenStream,
    mut item: syn::ItemImpl,
) -> Result<ParseOutput, Diagnostic> {
    let opts: BindgenAttrs = syn::parse2(attr)?;

    if item.defaultness.is_some() {
        return Err(Diagnostic::spanned_error(
            &item.defaultness,
            "#[wasm_bindgen] default impls are not supported",
        ));
    }
    if item.unsafety.is_some() {
        return Err(Diagnostic::spanned_error(
            &item.unsafety,
            "#[wasm_bindgen] unsafe impls are not supported",
        ));
    }
    if let Some((_, path, _)) = &item.trait_ {
        return Err(Diagnostic::spanned_error(
            path,
            "#[wasm_bindgen] trait impls are not supported",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(Diagnostic::spanned_error(
            &item.generics,
            "#[wasm_bindgen] generic impls aren't supported",
        ));
    }

    let class_path = match item.self_ty.as_ref() {
        syn::Type::Path(path) if path.qself.is_none() => &path.path,
        other => {
            return Err(Diagnostic::spanned_error(
                other,
                "unsupported self type in #[wasm_bindgen] impl",
            ));
        }
    };
    let class = class_path
        .segments
        .last()
        .ok_or_else(|| Diagnostic::spanned_error(class_path, "expected an impl self type"))?
        .ident
        .clone();

    let mut program = ast::Program::default();
    if let Some(path) = opts.wasm_bindgen() {
        program.wasm_bindgen = path.clone();
    }
    if let Some(path) = opts.wasm_bindgen_futures() {
        program.wasm_bindgen_futures = path.clone();
    }
    if let Some(path) = opts.js_sys() {
        program.js_sys = path.clone();
    }

    let marker = ClassMarker {
        class: class.clone(),
        js_class: opts
            .js_class()
            .map(|s| s.0.to_string())
            .unwrap_or_else(|| class.unraw().to_string()),
        js_namespace: opts.js_namespace().map(|(ns, _)| ns.0),
        wasm_bindgen: program.wasm_bindgen.clone(),
        wasm_bindgen_futures: program.wasm_bindgen_futures.clone(),
        js_sys: program.js_sys.clone(),
    };

    let mut errors = Vec::new();
    for impl_item in item.items.iter_mut() {
        match impl_item {
            syn::ImplItem::Fn(method) => {
                if let Err(error) = method.macro_parse(&mut program, &marker) {
                    errors.push(error);
                }
            }
            syn::ImplItem::Const(_) => errors.push(Diagnostic::spanned_error(
                impl_item,
                "const definitions aren't supported with #[wasm_bindgen]",
            )),
            syn::ImplItem::Type(_) => errors.push(Diagnostic::spanned_error(
                impl_item,
                "type definitions in impls aren't supported with #[wasm_bindgen]",
            )),
            syn::ImplItem::Macro(_) => errors.push(Diagnostic::spanned_error(
                impl_item,
                "macros in impls aren't supported",
            )),
            syn::ImplItem::Verbatim(_) => panic!("unparsed impl item?"),
            other => errors.push(Diagnostic::spanned_error(
                other,
                "failed to parse this item as a known item",
            )),
        }
    }
    Diagnostic::from_vec(errors)?;
    opts.check_used();
    parser::unused_attrs_diagnostic()?;

    Ok(ParseOutput {
        program,
        tokens: item.to_token_stream(),
        main: None,
    })
}

/// Parse a struct body through the upstream exported-struct converter.
pub fn parse_struct(input: TokenStream) -> Result<ast::Struct, Diagnostic> {
    parser::reset_attrs_used();

    let mut item = syn::parse2::<syn::ItemStruct>(input)?;
    let program = ast::Program::default();
    let parsed = (&mut item).convert(&program)?;
    parser::unused_attrs_diagnostic()?;

    Ok(parsed)
}

/// Parse an internal class marker method into the upstream macro-support AST.
pub fn parse_class_marker(
    attr: TokenStream,
    input: TokenStream,
) -> Result<ast::Program, Diagnostic> {
    Ok(parse_class_marker_with_tokens(attr, input)?.program)
}

/// Parse an internal class marker method into the upstream macro-support AST
/// and return the method tokens preserved by the upstream parser.
pub fn parse_class_marker_with_tokens(
    attr: TokenStream,
    input: TokenStream,
) -> Result<ParseOutput, Diagnostic> {
    parser::reset_attrs_used();

    let mut item = syn::parse2::<syn::ImplItemFn>(input)?;
    let opts: ClassMarker = syn::parse2(attr)?;
    let mut program = ast::Program::default();
    item.macro_parse(&mut program, &opts)?;
    parser::unused_attrs_diagnostic()?;

    Ok(ParseOutput {
        program,
        tokens: item.to_token_stream(),
        main: None,
    })
}

/// Parse a `wasm_bindgen::link_to!` input into its module AST.
pub fn parse_link_to(input: TokenStream) -> Result<ast::LinkToModule, Diagnostic> {
    parser::reset_attrs_used();

    let opts = syn::parse2(input)?;
    let parsed = parser::link_to(opts)?;
    parser::unused_attrs_diagnostic()?;

    Ok(parsed)
}

/// Takes the parsed input from a `#[wasm_bindgen]` macro and returns the generated bindings
#[cfg(feature = "expand")]
pub fn expand(attr: TokenStream, input: TokenStream) -> Result<TokenStream, Diagnostic> {
    parser::reset_attrs_used();
    // if struct is encountered, add `derive` attribute and let everything happen there (workaround
    // to help parsing cfg_attr correctly).
    let item = syn::parse2::<syn::Item>(input)?;
    if let syn::Item::Struct(mut s) = item {
        let opts: BindgenAttrs = syn::parse2(attr.clone())?;
        let wasm_bindgen = opts
            .wasm_bindgen()
            .cloned()
            .unwrap_or_else(|| syn::parse_quote! { ::wasm_bindgen });

        // Inject `parent: <wasm_bindgen>::Parent<Parent>` when the struct
        // declares `#[wasm_bindgen(extends = Parent)]`, so users never write
        // the field themselves. Also rejects user-declared `Parent<T>` fields.
        let extends_path = opts.attrs.iter().find_map(|(_, a)| match a {
            parser::BindgenAttr::Extends(_, path) => Some(path.clone()),
            _ => None,
        });
        parser::inject_parent_field(&mut s, extends_path.as_ref(), &wasm_bindgen)?;

        let item = quote! {
            #[derive(#wasm_bindgen::__rt::BindgenedStruct)]
            #[wasm_bindgen(#attr)]
            #s
        };
        return Ok(item);
    }

    let opts = syn::parse2(attr)?;
    let mut tokens = proc_macro2::TokenStream::new();
    let mut program = ast::Program::default();
    item.macro_parse(&mut program, (Some(opts), &mut tokens))?;
    program.try_to_tokens(&mut tokens)?;

    // If we successfully got here then we should have used up all attributes
    // and considered all of them to see if they were used. If one was forgotten
    // that's a bug on our end, so sanity check here.
    parser::check_unused_attrs(&mut tokens);

    Ok(tokens)
}

/// Takes the parsed input from a `wasm_bindgen::link_to` macro and returns the generated link
#[cfg(feature = "expand")]
pub fn expand_link_to(input: TokenStream) -> Result<TokenStream, Diagnostic> {
    parser::reset_attrs_used();
    let opts = syn::parse2(input)?;

    let mut tokens = proc_macro2::TokenStream::new();
    let link = parser::link_to(opts)?;
    link.try_to_tokens(&mut tokens)?;

    Ok(tokens)
}

/// Takes the parsed input from a `#[wasm_bindgen]` macro and returns the generated bindings
#[cfg(feature = "expand")]
pub fn expand_class_marker(
    attr: TokenStream,
    input: TokenStream,
) -> Result<TokenStream, Diagnostic> {
    parser::reset_attrs_used();
    let mut item = syn::parse2::<syn::ImplItemFn>(input)?;
    let opts: ClassMarker = syn::parse2(attr)?;

    let mut program = ast::Program::default();
    item.macro_parse(&mut program, &opts)?;

    // This is where things are slightly different, we are being expanded in the
    // context of an impl so we can't inject arbitrary item-like tokens into the
    // output stream. If we were to do that then it wouldn't parse!
    //
    // Instead what we want to do is to generate the tokens for `program` into
    // the header of the function. This'll inject some no_mangle functions and
    // statics and such, and they should all be valid in the context of the
    // start of a function.
    //
    // We manually implement `ToTokens for ImplItemFn` here, injecting our
    // program's tokens before the actual method's inner body tokens.
    let mut tokens = proc_macro2::TokenStream::new();
    tokens.append_all(
        item.attrs
            .iter()
            .filter(|attr| matches!(attr.style, syn::AttrStyle::Outer)),
    );
    item.vis.to_tokens(&mut tokens);
    item.sig.to_tokens(&mut tokens);
    let mut err = None;
    item.block.brace_token.surround(&mut tokens, |tokens| {
        if let Err(e) = program.try_to_tokens(tokens) {
            err = Some(e);
        }
        parser::check_unused_attrs(tokens); // same as above
        tokens.append_all(
            item.attrs
                .iter()
                .filter(|attr| matches!(attr.style, syn::AttrStyle::Inner(_))),
        );
        tokens.append_all(&item.block.stmts);
    });

    if let Some(err) = err {
        return Err(err);
    }

    Ok(tokens)
}

struct ClassMarker {
    class: syn::Ident,
    js_class: String,
    js_namespace: Option<Vec<String>>,
    wasm_bindgen: syn::Path,
    wasm_bindgen_futures: syn::Path,
    js_sys: syn::Path,
}

impl Parse for ClassMarker {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let class = input.parse::<syn::Ident>()?;
        input.parse::<Token![=]>()?;
        let mut js_class = input.parse::<syn::LitStr>()?.value();
        js_class = js_class
            .strip_prefix("r#")
            .map(String::from)
            .unwrap_or(js_class);

        let mut js_namespace: Option<Vec<String>> = None;
        let mut wasm_bindgen = None;
        let mut wasm_bindgen_futures = None;
        let mut js_sys = None;

        loop {
            if input.parse::<Option<Token![,]>>()?.is_some() {
                let ident = input.parse::<syn::Ident>()?;

                if ident == "js_namespace" {
                    if js_namespace.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "found duplicate `js_namespace`",
                        ));
                    }
                    input.parse::<Token![=]>()?;
                    let content;
                    syn::bracketed!(content in input);
                    let segs: syn::punctuated::Punctuated<syn::LitStr, Token![,]> = content
                        .parse_terminated(|p: ParseStream| p.parse::<syn::LitStr>(), Token![,])?;
                    js_namespace = Some(segs.into_iter().map(|s| s.value()).collect());
                } else if ident == "wasm_bindgen" {
                    if wasm_bindgen.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "found duplicate `wasm_bindgen`",
                        ));
                    }

                    input.parse::<Token![=]>()?;
                    wasm_bindgen = Some(input.parse::<syn::Path>()?);
                } else if ident == "wasm_bindgen_futures" {
                    if wasm_bindgen_futures.is_some() {
                        return Err(syn::Error::new(
                            ident.span(),
                            "found duplicate `wasm_bindgen_futures`",
                        ));
                    }

                    input.parse::<Token![=]>()?;
                    wasm_bindgen_futures = Some(input.parse::<syn::Path>()?);
                } else if ident == "js_sys" {
                    if js_sys.is_some() {
                        return Err(syn::Error::new(ident.span(), "found duplicate `js_sys`"));
                    }

                    input.parse::<Token![=]>()?;
                    js_sys = Some(input.parse::<syn::Path>()?);
                } else {
                    return Err(syn::Error::new(
                        ident.span(),
                        "expected `js_namespace`, `wasm_bindgen`, `wasm_bindgen_futures`, or `js_sys`",
                    ));
                }
            } else {
                break;
            }
        }

        Ok(ClassMarker {
            class,
            js_class,
            js_namespace,
            wasm_bindgen: wasm_bindgen.unwrap_or_else(|| syn::parse_quote! { wasm_bindgen }),
            wasm_bindgen_futures: wasm_bindgen_futures
                .unwrap_or_else(|| syn::parse_quote! { wasm_bindgen_futures }),
            js_sys: js_sys.unwrap_or_else(|| syn::parse_quote! { js_sys }),
        })
    }
}

#[cfg(feature = "expand")]
pub fn expand_struct_marker(item: TokenStream) -> Result<TokenStream, Diagnostic> {
    parser::reset_attrs_used();

    let mut s: syn::ItemStruct = syn::parse2(item)?;

    let mut program = ast::Program::default();
    program.structs.push((&mut s).convert(&program)?);

    let mut tokens = proc_macro2::TokenStream::new();
    program.try_to_tokens(&mut tokens)?;

    parser::check_unused_attrs(&mut tokens);

    Ok(tokens)
}
