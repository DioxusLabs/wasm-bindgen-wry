use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::Ident;
use wasm_bindgen_macro_support::ast::{ImportFunction, ImportFunctionKind, MethodKind};

pub(super) struct GeneratedArgs {
    /// Function parameter declarations: `arg1: T1, arg2: T2`
    pub(super) fn_params: TokenStream,
    /// Individual JS call fn pointer type tokens, with function generics erased.
    pub(super) fn_type_list: Vec<TokenStream>,
    /// Individual values to pass to the JS call, transmuted to erased types when needed.
    pub(super) call_value_list: Vec<TokenStream>,
}

pub(super) struct GenericEraseContext {
    type_params: HashMap<String, Option<syn::Type>>,
    lifetimes: HashSet<String>,
}

impl GenericEraseContext {
    pub(super) fn new(func: &ImportFunction) -> Self {
        Self {
            type_params: func
                .generics
                .type_params()
                .map(|param| (param.ident.to_string(), param.default.clone()))
                .collect(),
            lifetimes: func
                .generics
                .lifetimes()
                .map(|param| param.lifetime.ident.to_string())
                .collect(),
        }
    }

    pub(super) fn type_uses_erased_params(&self, ty: &syn::Type) -> bool {
        let known_type_params = self.type_params.keys().cloned().collect();
        let mut found_type_params = HashSet::new();
        collect_type_params(ty, &known_type_params, &mut found_type_params);

        let mut found_lifetimes = HashSet::new();
        collect_lifetime_params(ty, &self.lifetimes, &mut found_lifetimes);

        !found_type_params.is_empty() || !found_lifetimes.is_empty()
    }

    pub(super) fn concrete_type(&self, ty: &syn::Type, krate: &TokenStream) -> syn::Type {
        let mut ty = ty.clone();
        erase_type_params(&mut ty, self, krate);
        ty
    }
}

pub(super) fn import_function_is_instance_method(func: &ImportFunction) -> bool {
    matches!(
        &func.kind,
        ImportFunctionKind::Method {
            kind: MethodKind::Operation(operation),
            ..
        } if !operation.is_static
    )
}

pub(super) fn function_argument_parts(
    argument: &wasm_bindgen_macro_support::ast::FunctionArgumentData,
) -> syn::Result<(&Ident, &syn::Type)> {
    let syn::Pat::Ident(ident) = argument.pat_type.pat.as_ref() else {
        return Err(syn::Error::new_spanned(
            &argument.pat_type.pat,
            "complex patterns are not supported by wry-bindgen codegen",
        ));
    };
    Ok((&ident.ident, argument.pat_type.ty.as_ref()))
}

/// Generate argument lists
pub(super) fn generate_args(
    func: &ImportFunction,
    krate: &TokenStream,
) -> syn::Result<GeneratedArgs> {
    let mut fn_params = Vec::new();
    let mut fn_types = Vec::new();
    let mut call_values = Vec::new();
    let span = func.rust_name.span();
    let erase = GenericEraseContext::new(func);

    let skip_receiver = import_function_is_instance_method(func);
    if skip_receiver {
        fn_types.push(quote_spanned! {span=> &#krate::JsValue });
        call_values.push(quote_spanned! {span=> &self.obj });
    }

    // Add explicit arguments
    for arg in func
        .function
        .arguments
        .iter()
        .skip(usize::from(skip_receiver))
    {
        let (name, ty) = function_argument_parts(arg)?;
        fn_params.push(quote_spanned! {span=> #name: #ty });
        if erase.type_uses_erased_params(ty) {
            let concrete_ty = erase.concrete_type(ty, krate);
            fn_types.push(quote_spanned! {span=> #concrete_ty });
            call_values.push(quote_spanned! {span=>
                unsafe {
                    #krate::__rt::core::mem::transmute_copy(
                        &#krate::__rt::core::mem::ManuallyDrop::new(#name)
                    )
                }
            });
        } else {
            fn_types.push(quote_spanned! {span=> #ty });
            call_values.push(quote_spanned! {span=> #name });
        }
    }

    let fn_params_tokens = if fn_params.is_empty() {
        quote_spanned! {span=>}
    } else {
        quote_spanned! {span=> #(#fn_params),* }
    };

    Ok(GeneratedArgs {
        fn_params: fn_params_tokens,
        fn_type_list: fn_types,
        call_value_list: call_values,
    })
}

pub(super) fn receiver_impl_type(ty: &syn::Type) -> syn::Result<syn::Type> {
    match ty {
        syn::Type::Reference(r) => receiver_impl_type(&r.elem),
        syn::Type::Path(_) => Ok(ty.clone()),
        _ => Err(syn::Error::new_spanned(ty, "unsupported receiver type")),
    }
}

pub(super) fn add_static_bounds(generics: &syn::Generics) -> syn::Generics {
    let mut generics = generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(syn::parse_quote!('static));
    }
    generics
}

pub(super) fn add_js_call_bounds(
    func: &ImportFunction,
    krate: &TokenStream,
    include_ret: bool,
) -> syn::Generics {
    let mut generics = func.generics.clone();
    add_js_call_bounds_to_generics(&mut generics, func, krate, include_ret);
    generics
}

pub(super) fn add_js_call_bounds_to_generics(
    generics: &mut syn::Generics,
    func: &ImportFunction,
    krate: &TokenStream,
    include_ret: bool,
) {
    let erase = GenericEraseContext::new(func);
    let known_type_params: std::collections::HashSet<String> = func
        .generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect();

    let skip_receiver = import_function_is_instance_method(func);
    for arg in func
        .function
        .arguments
        .iter()
        .skip(usize::from(skip_receiver))
    {
        let arg_ty = arg.pat_type.ty.as_ref();
        if erase.type_uses_erased_params(arg_ty) {
            let concrete_ty = erase.concrete_type(arg_ty, krate);
            push_erasure_bound(generics, arg_ty, &concrete_ty, krate, false);
        }
        if type_uses_type_params(arg_ty, &known_type_params) {
            push_arg_type_bounds(generics, arg_ty, krate);
        }
    }

    if include_ret && let Some(ret) = &func.function.ret {
        let ret = &ret.r#type;
        if erase.type_uses_erased_params(ret) {
            let concrete_ty = erase.concrete_type(ret, krate);
            push_erasure_bound(generics, ret, &concrete_ty, krate, true);
        }
        if type_uses_type_params(ret, &known_type_params) {
            push_type_bound(
                generics,
                ret,
                &[
                    quote! { #krate::__rt::EncodeTypeDef },
                    quote! { #krate::__rt::BatchableResult },
                ],
            );
        }
    }
}

fn type_uses_type_params(ty: &syn::Type, known: &std::collections::HashSet<String>) -> bool {
    let mut found = std::collections::HashSet::new();
    collect_type_params(ty, known, &mut found);
    !found.is_empty()
}

fn push_arg_type_bounds(generics: &mut syn::Generics, ty: &syn::Type, krate: &TokenStream) {
    match ty {
        syn::Type::Reference(reference) => match &*reference.elem {
            syn::Type::Slice(slice) => {
                push_type_bound(
                    generics,
                    &slice.elem,
                    &[
                        quote! { #krate::__rt::EncodeTypeDef },
                        quote! { #krate::JsGeneric },
                    ],
                );
            }
            syn::Type::Path(path) if path_is_scoped_closure(path) => {}
            syn::Type::Path(path) if path.path.segments.len() == 1 => {
                let elem = &reference.elem;
                push_type_bound(generics, elem, &[quote! { #krate::__rt::EncodeTypeDef }]);
                push_reference_binary_encode_bound(generics, reference, krate);
            }
            _ => {
                push_type_bound(
                    generics,
                    ty,
                    &[
                        quote! { #krate::__rt::EncodeTypeDef },
                        quote! { #krate::__rt::BinaryEncode },
                    ],
                );
            }
        },
        _ => {
            push_type_bound(
                generics,
                ty,
                &[
                    quote! { #krate::__rt::EncodeTypeDef },
                    quote! { #krate::__rt::BinaryEncode },
                ],
            );
        }
    }
}

fn push_erasure_bound(
    generics: &mut syn::Generics,
    ty: &syn::Type,
    concrete_ty: &syn::Type,
    krate: &TokenStream,
    owned: bool,
) {
    let predicate = if !owned
        && let syn::Type::Reference(reference) = ty
        && let syn::Type::Reference(concrete_reference) = concrete_ty
    {
        let elem = &reference.elem;
        let concrete_elem = &concrete_reference.elem;
        if reference.mutability.is_some() {
            syn::parse_quote! {
                #elem: #krate::__rt::marker::ErasableGenericBorrowMut<#concrete_elem>
            }
        } else {
            syn::parse_quote! {
                #elem: #krate::__rt::marker::ErasableGenericBorrow<#concrete_elem>
            }
        }
    } else {
        syn::parse_quote! {
            #ty: #krate::__rt::marker::ErasableGenericOwn<#concrete_ty>
        }
    };
    generics.make_where_clause().predicates.push(predicate);
}

fn path_is_scoped_closure(path: &syn::TypePath) -> bool {
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "ScopedClosure" || segment.ident == "Closure")
}

fn push_reference_binary_encode_bound(
    generics: &mut syn::Generics,
    reference: &syn::TypeReference,
    krate: &TokenStream,
) {
    let elem = &reference.elem;
    let predicate = if let Some(lifetime) = &reference.lifetime {
        let mutability = &reference.mutability;
        syn::parse_quote! {
            &#lifetime #mutability #elem: #krate::__rt::BinaryEncode
        }
    } else {
        let mutability = &reference.mutability;
        syn::parse_quote! {
            for<'__wry_bindgen> &'__wry_bindgen #mutability #elem: #krate::__rt::BinaryEncode
        }
    };
    generics.make_where_clause().predicates.push(predicate);
}

fn push_type_bound(generics: &mut syn::Generics, ty: &syn::Type, bounds: &[TokenStream]) {
    let bounds = quote! { #(#bounds)+* };
    let predicate = match ty {
        syn::Type::Reference(reference) if reference.lifetime.is_none() => {
            let mutability = &reference.mutability;
            let elem = &reference.elem;
            syn::parse_quote! {
                for<'__wry_bindgen> &'__wry_bindgen #mutability #elem: #bounds
            }
        }
        _ => syn::parse_quote! {
            #ty: #bounds
        },
    };
    generics.make_where_clause().predicates.push(predicate);
}

pub(super) fn split_method_generics(
    generics: &syn::Generics,
    receiver: &syn::Type,
) -> (syn::Generics, syn::Generics) {
    let known_type_params: std::collections::HashSet<String> = generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect();
    let mut receiver_type_params = std::collections::HashSet::new();
    collect_type_params(receiver, &known_type_params, &mut receiver_type_params);
    let mut changed = true;
    while changed {
        changed = false;
        for param in generics.type_params() {
            if receiver_type_params.contains(&param.ident.to_string()) {
                let before = receiver_type_params.len();
                collect_type_params_from_bounds(
                    &param.bounds,
                    &known_type_params,
                    &mut receiver_type_params,
                );
                if let Some(default) = &param.default {
                    collect_type_params(default, &known_type_params, &mut receiver_type_params);
                }
                changed |= receiver_type_params.len() != before;
            }
        }
    }

    let mut impl_generics = generics.clone();
    impl_generics.params = generics
        .params
        .iter()
        .filter(|param| match param {
            syn::GenericParam::Type(param) => {
                receiver_type_params.contains(&param.ident.to_string())
            }
            syn::GenericParam::Lifetime(_) | syn::GenericParam::Const(_) => false,
        })
        .cloned()
        .collect();
    impl_generics.where_clause = None;

    let mut method_generics = generics.clone();
    method_generics.params = generics
        .params
        .iter()
        .filter(|param| match param {
            syn::GenericParam::Type(param) => {
                !receiver_type_params.contains(&param.ident.to_string())
            }
            syn::GenericParam::Lifetime(_) | syn::GenericParam::Const(_) => true,
        })
        .cloned()
        .collect();

    (impl_generics, method_generics)
}

fn collect_type_params(
    ty: &syn::Type,
    known: &std::collections::HashSet<String>,
    found: &mut std::collections::HashSet<String>,
) {
    match ty {
        syn::Type::Reference(reference) => collect_type_params(&reference.elem, known, found),
        syn::Type::Path(path) => {
            if let Some(qself) = &path.qself {
                collect_type_params(&qself.ty, known, found);
            }
            for segment in &path.path.segments {
                let ident = segment.ident.to_string();
                if known.contains(&ident) {
                    found.insert(ident);
                }
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        match arg {
                            syn::GenericArgument::Type(ty) => collect_type_params(ty, known, found),
                            syn::GenericArgument::AssocType(assoc) => {
                                collect_type_params(&assoc.ty, known, found);
                            }
                            _ => {}
                        }
                    }
                } else if let syn::PathArguments::Parenthesized(args) = &segment.arguments {
                    for input in &args.inputs {
                        collect_type_params(input, known, found);
                    }
                    if let syn::ReturnType::Type(_, output) = &args.output {
                        collect_type_params(output, known, found);
                    }
                }
            }
        }
        syn::Type::TraitObject(trait_object) => {
            for bound in &trait_object.bounds {
                collect_type_params_from_bound(bound, known, found);
            }
        }
        syn::Type::BareFn(function) => {
            for input in &function.inputs {
                collect_type_params(&input.ty, known, found);
            }
            if let syn::ReturnType::Type(_, output) = &function.output {
                collect_type_params(output, known, found);
            }
        }
        syn::Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_type_params(elem, known, found);
            }
        }
        syn::Type::Paren(paren) => collect_type_params(&paren.elem, known, found),
        syn::Type::Group(group) => collect_type_params(&group.elem, known, found),
        syn::Type::Slice(slice) => collect_type_params(&slice.elem, known, found),
        syn::Type::Array(array) => collect_type_params(&array.elem, known, found),
        syn::Type::Ptr(ptr) => collect_type_params(&ptr.elem, known, found),
        _ => {}
    }
}

fn erase_type_params(ty: &mut syn::Type, context: &GenericEraseContext, krate: &TokenStream) {
    match ty {
        syn::Type::Reference(reference) => {
            if let Some(lifetime) = &mut reference.lifetime
                && context.lifetimes.contains(&lifetime.ident.to_string())
            {
                *lifetime = syn::Lifetime::new("'static", lifetime.span());
            }
            erase_type_params(&mut reference.elem, context, krate);
        }
        syn::Type::Path(path) => {
            if let Some(qself) = &mut path.qself {
                erase_type_params(&mut qself.ty, context, krate);
            }

            if path.qself.is_none()
                && let Some(first_segment) = path.path.segments.first()
            {
                let ident = first_segment.ident.to_string();
                if let Some(default) = context.type_params.get(&ident) {
                    if let Some(default) = default {
                        if path.path.segments.len() == 1 {
                            *ty = default.clone();
                            erase_type_params(ty, context, krate);
                            return;
                        }
                        if let syn::Type::Path(default_path) = default {
                            let remaining: Vec<_> =
                                path.path.segments.iter().skip(1).cloned().collect();
                            path.path.segments = default_path.path.segments.clone();
                            path.path.segments.extend(remaining);
                        } else {
                            *ty = default.clone();
                            erase_type_params(ty, context, krate);
                            return;
                        }
                    } else {
                        *ty = syn::parse_quote!(#krate::JsValue);
                        return;
                    }
                }
            }

            if let syn::Type::Path(path) = ty {
                for segment in &mut path.path.segments {
                    erase_path_arguments(&mut segment.arguments, context, krate);
                }
            }
        }
        syn::Type::TraitObject(trait_object) => {
            for bound in &mut trait_object.bounds {
                erase_type_param_bound(bound, context, krate);
            }
        }
        syn::Type::BareFn(function) => {
            if let Some(lifetimes) = &mut function.lifetimes {
                for param in &mut lifetimes.lifetimes {
                    if let syn::GenericParam::Lifetime(param) = param
                        && context
                            .lifetimes
                            .contains(&param.lifetime.ident.to_string())
                    {
                        param.lifetime = syn::Lifetime::new("'static", param.lifetime.span());
                    }
                }
            }
            for input in &mut function.inputs {
                erase_type_params(&mut input.ty, context, krate);
            }
            if let syn::ReturnType::Type(_, output) = &mut function.output {
                erase_type_params(output, context, krate);
            }
        }
        syn::Type::Tuple(tuple) => {
            for elem in &mut tuple.elems {
                erase_type_params(elem, context, krate);
            }
        }
        syn::Type::Paren(paren) => erase_type_params(&mut paren.elem, context, krate),
        syn::Type::Group(group) => erase_type_params(&mut group.elem, context, krate),
        syn::Type::Slice(slice) => erase_type_params(&mut slice.elem, context, krate),
        syn::Type::Array(array) => erase_type_params(&mut array.elem, context, krate),
        syn::Type::Ptr(ptr) => erase_type_params(&mut ptr.elem, context, krate),
        _ => {}
    }
}

fn erase_path_arguments(
    arguments: &mut syn::PathArguments,
    context: &GenericEraseContext,
    krate: &TokenStream,
) {
    match arguments {
        syn::PathArguments::AngleBracketed(args) => {
            for arg in &mut args.args {
                match arg {
                    syn::GenericArgument::Lifetime(lifetime) => {
                        if context.lifetimes.contains(&lifetime.ident.to_string()) {
                            *lifetime = syn::Lifetime::new("'static", lifetime.span());
                        }
                    }
                    syn::GenericArgument::Type(ty) => erase_type_params(ty, context, krate),
                    syn::GenericArgument::AssocType(assoc) => {
                        erase_type_params(&mut assoc.ty, context, krate);
                    }
                    syn::GenericArgument::Constraint(constraint) => {
                        for bound in &mut constraint.bounds {
                            erase_type_param_bound(bound, context, krate);
                        }
                    }
                    _ => {}
                }
            }
        }
        syn::PathArguments::Parenthesized(args) => {
            for input in &mut args.inputs {
                erase_type_params(input, context, krate);
            }
            if let syn::ReturnType::Type(_, output) = &mut args.output {
                erase_type_params(output, context, krate);
            }
        }
        syn::PathArguments::None => {}
    }
}

fn erase_type_param_bound(
    bound: &mut syn::TypeParamBound,
    context: &GenericEraseContext,
    krate: &TokenStream,
) {
    match bound {
        syn::TypeParamBound::Lifetime(lifetime) => {
            if context.lifetimes.contains(&lifetime.ident.to_string()) {
                *lifetime = syn::Lifetime::new("'static", lifetime.span());
            }
        }
        syn::TypeParamBound::Trait(trait_bound) => {
            for segment in &mut trait_bound.path.segments {
                erase_path_arguments(&mut segment.arguments, context, krate);
            }
        }
        _ => {}
    }
}

fn collect_lifetime_params(ty: &syn::Type, known: &HashSet<String>, found: &mut HashSet<String>) {
    match ty {
        syn::Type::Reference(reference) => {
            if let Some(lifetime) = &reference.lifetime
                && known.contains(&lifetime.ident.to_string())
            {
                found.insert(lifetime.ident.to_string());
            }
            collect_lifetime_params(&reference.elem, known, found);
        }
        syn::Type::Path(path) => {
            if let Some(qself) = &path.qself {
                collect_lifetime_params(&qself.ty, known, found);
            }
            for segment in &path.path.segments {
                collect_lifetime_params_from_path_arguments(&segment.arguments, known, found);
            }
        }
        syn::Type::TraitObject(trait_object) => {
            for bound in &trait_object.bounds {
                collect_lifetime_params_from_bound(bound, known, found);
            }
        }
        syn::Type::BareFn(function) => {
            if let Some(lifetimes) = &function.lifetimes {
                for param in &lifetimes.lifetimes {
                    if let syn::GenericParam::Lifetime(param) = param
                        && known.contains(&param.lifetime.ident.to_string())
                    {
                        found.insert(param.lifetime.ident.to_string());
                    }
                }
            }
            for input in &function.inputs {
                collect_lifetime_params(&input.ty, known, found);
            }
            if let syn::ReturnType::Type(_, output) = &function.output {
                collect_lifetime_params(output, known, found);
            }
        }
        syn::Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_lifetime_params(elem, known, found);
            }
        }
        syn::Type::Paren(paren) => collect_lifetime_params(&paren.elem, known, found),
        syn::Type::Group(group) => collect_lifetime_params(&group.elem, known, found),
        syn::Type::Slice(slice) => collect_lifetime_params(&slice.elem, known, found),
        syn::Type::Array(array) => collect_lifetime_params(&array.elem, known, found),
        syn::Type::Ptr(ptr) => collect_lifetime_params(&ptr.elem, known, found),
        _ => {}
    }
}

fn collect_lifetime_params_from_path_arguments(
    arguments: &syn::PathArguments,
    known: &HashSet<String>,
    found: &mut HashSet<String>,
) {
    match arguments {
        syn::PathArguments::AngleBracketed(args) => {
            for arg in &args.args {
                match arg {
                    syn::GenericArgument::Lifetime(lifetime) => {
                        if known.contains(&lifetime.ident.to_string()) {
                            found.insert(lifetime.ident.to_string());
                        }
                    }
                    syn::GenericArgument::Type(ty) => {
                        collect_lifetime_params(ty, known, found);
                    }
                    syn::GenericArgument::AssocType(assoc) => {
                        collect_lifetime_params(&assoc.ty, known, found);
                    }
                    syn::GenericArgument::Constraint(constraint) => {
                        for bound in &constraint.bounds {
                            collect_lifetime_params_from_bound(bound, known, found);
                        }
                    }
                    _ => {}
                }
            }
        }
        syn::PathArguments::Parenthesized(args) => {
            for input in &args.inputs {
                collect_lifetime_params(input, known, found);
            }
            if let syn::ReturnType::Type(_, output) = &args.output {
                collect_lifetime_params(output, known, found);
            }
        }
        syn::PathArguments::None => {}
    }
}

fn collect_lifetime_params_from_bound(
    bound: &syn::TypeParamBound,
    known: &HashSet<String>,
    found: &mut HashSet<String>,
) {
    match bound {
        syn::TypeParamBound::Lifetime(lifetime) => {
            if known.contains(&lifetime.ident.to_string()) {
                found.insert(lifetime.ident.to_string());
            }
        }
        syn::TypeParamBound::Trait(bound) => {
            for segment in &bound.path.segments {
                collect_lifetime_params_from_path_arguments(&segment.arguments, known, found);
            }
        }
        _ => {}
    }
}

pub(super) fn collect_constraining_type_params(
    ty: &syn::Type,
    known: &HashSet<String>,
    found: &mut HashSet<String>,
) -> bool {
    match ty {
        syn::Type::Reference(reference) => {
            collect_constraining_type_params(&reference.elem, known, found)
        }
        syn::Type::Path(path) => {
            if path.qself.is_some() {
                let mut qself_params = HashSet::new();
                collect_type_params(ty, known, &mut qself_params);
                return qself_params.is_empty();
            }

            if path.path.segments.len() == 1 {
                let segment = &path.path.segments[0];
                let ident = segment.ident.to_string();
                if known.contains(&ident) && segment.arguments.is_empty() {
                    found.insert(ident);
                    return true;
                }
            } else if path
                .path
                .segments
                .first()
                .is_some_and(|segment| known.contains(&segment.ident.to_string()))
            {
                return false;
            }

            for segment in &path.path.segments {
                match &segment.arguments {
                    syn::PathArguments::AngleBracketed(args) => {
                        for arg in &args.args {
                            match arg {
                                syn::GenericArgument::Type(ty) => {
                                    if !collect_constraining_type_params(ty, known, found) {
                                        return false;
                                    }
                                }
                                syn::GenericArgument::AssocType(assoc) => {
                                    let mut assoc_params = HashSet::new();
                                    collect_type_params(&assoc.ty, known, &mut assoc_params);
                                    if !assoc_params.is_empty() {
                                        return false;
                                    }
                                }
                                syn::GenericArgument::Constraint(constraint) => {
                                    let mut constraint_params = HashSet::new();
                                    collect_type_params_from_bounds(
                                        &constraint.bounds,
                                        known,
                                        &mut constraint_params,
                                    );
                                    if !constraint_params.is_empty() {
                                        return false;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    syn::PathArguments::Parenthesized(args) => {
                        let mut arg_params = HashSet::new();
                        for input in &args.inputs {
                            collect_type_params(input, known, &mut arg_params);
                        }
                        if let syn::ReturnType::Type(_, output) = &args.output {
                            collect_type_params(output, known, &mut arg_params);
                        }
                        if !arg_params.is_empty() {
                            return false;
                        }
                    }
                    syn::PathArguments::None => {}
                }
            }
            true
        }
        syn::Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .all(|elem| collect_constraining_type_params(elem, known, found)),
        syn::Type::Paren(paren) => collect_constraining_type_params(&paren.elem, known, found),
        syn::Type::Group(group) => collect_constraining_type_params(&group.elem, known, found),
        syn::Type::Slice(slice) => collect_constraining_type_params(&slice.elem, known, found),
        syn::Type::Array(array) => collect_constraining_type_params(&array.elem, known, found),
        syn::Type::Ptr(ptr) => collect_constraining_type_params(&ptr.elem, known, found),
        syn::Type::TraitObject(_) | syn::Type::BareFn(_) => {
            let mut params = HashSet::new();
            collect_type_params(ty, known, &mut params);
            params.is_empty()
        }
        _ => true,
    }
}

fn collect_type_params_from_bounds(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
    known: &std::collections::HashSet<String>,
    found: &mut std::collections::HashSet<String>,
) {
    for bound in bounds {
        collect_type_params_from_bound(bound, known, found);
    }
}

fn collect_type_params_from_bound(
    bound: &syn::TypeParamBound,
    known: &std::collections::HashSet<String>,
    found: &mut std::collections::HashSet<String>,
) {
    if let syn::TypeParamBound::Trait(bound) = bound {
        for segment in &bound.path.segments {
            match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => {
                    for arg in &args.args {
                        match arg {
                            syn::GenericArgument::Type(ty) => {
                                collect_type_params(ty, known, found);
                            }
                            syn::GenericArgument::AssocType(assoc) => {
                                collect_type_params(&assoc.ty, known, found);
                            }
                            syn::GenericArgument::Constraint(constraint) => {
                                collect_type_params_from_bounds(&constraint.bounds, known, found);
                            }
                            _ => {}
                        }
                    }
                }
                syn::PathArguments::Parenthesized(args) => {
                    for input in &args.inputs {
                        collect_type_params(input, known, found);
                    }
                    if let syn::ReturnType::Type(_, output) = &args.output {
                        collect_type_params(output, known, found);
                    }
                }
                syn::PathArguments::None => {}
            }
        }
    }
}
