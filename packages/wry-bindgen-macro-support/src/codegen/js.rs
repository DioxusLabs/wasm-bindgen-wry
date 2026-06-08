use wasm_bindgen_macro_support::ast::{
    ImportFunction, ImportFunctionKind, MethodKind, OperationKind,
};

use super::erasure::import_function_is_instance_method;

fn generate_vendor_prefixed_constructor(class: &str, prefixes: &[String], prefix: &str) -> String {
    let mut result = format!("(typeof {prefix}{class} !== 'undefined' ? {prefix}{class} : ");

    for (i, vendor_prefix) in prefixes.iter().enumerate() {
        let prefixed_class = format!("{vendor_prefix}{class}");
        if i == prefixes.len() - 1 {
            result.push_str(&format!(
                "(typeof {prefix}{prefixed_class} !== 'undefined' ? {prefix}{prefixed_class} : undefined)"
            ));
        } else {
            result.push_str(&format!(
                "(typeof {prefix}{prefixed_class} !== 'undefined' ? {prefix}{prefixed_class} : "
            ));
        }
    }

    // Close the opening paren plus the still-open paren of every non-final
    // prefix branch (each adds a nested ternary). The final branch closed its
    // own paren above. Without this, two or more prefixes produce unbalanced
    // parentheses and the whole generated bundle is a syntax error.
    result.push_str(&")".repeat(prefixes.len()));
    result
}

/// Generate JavaScript code for the function.
pub(super) fn generate_js_code(
    func: &ImportFunction,
    js_namespace: Option<&[String]>,
    vendor_prefixes: &std::collections::HashMap<String, Vec<String>>,
    prefix: &str,
    skip_catch_wrapper: bool,
) -> String {
    let js_name = &func.function.name;
    let prefix = namespace_prefix(prefix, js_namespace);
    let explicit_arg_count =
        func.function.arguments.len() - usize::from(import_function_is_instance_method(func));

    let args: Vec<_> = (0..explicit_arg_count).map(|i| format!("a{i}")).collect();
    let args_str = args.join(", ");
    let spread_args = |args: &[String]| -> String {
        if func.variadic && !args.is_empty() {
            let last = args.last().unwrap();
            if args.len() == 1 {
                format!("...{last}")
            } else {
                format!("{}, ...{last}", args[..args.len() - 1].join(", "))
            }
        } else {
            args.join(", ")
        }
    };
    let call_args_str = spread_args(&args);

    // A `final` (non-`structural`) namespaced free function resolves its callee
    // once, when the binding is created, rather than re-reading the property on
    // every call. This matches wasm-bindgen's `final`: the function "never
    // changes after it was imported", so later reassignment of the property is
    // not observed. Only applies when there is an object to read from.
    let mut hoisted_callee: Option<String> = None;

    let (params, body) = match &func.kind {
        ImportFunctionKind::Normal => {
            let callee = if prefix.is_empty() {
                js_name.to_string()
            } else {
                let object = prefix.trim_end_matches('.');
                js_property_access(object, js_name)
            };
            if !func.structural && !prefix.is_empty() {
                hoisted_callee = Some(callee);
                (
                    format!("({args_str})"),
                    format!("__wry_callee({call_args_str})"),
                )
            } else {
                (
                    format!("({args_str})"),
                    format!("{callee}({call_args_str})"),
                )
            }
        }
        ImportFunctionKind::Method {
            class,
            kind: MethodKind::Constructor,
            ..
        } => {
            let body = if let Some(prefixes) = vendor_prefixes.get(class) {
                if prefixes.is_empty() {
                    format!("new {prefix}{class}({call_args_str})")
                } else {
                    let constructor_expr =
                        generate_vendor_prefixed_constructor(class, prefixes, &prefix);
                    format!("new ({constructor_expr})({call_args_str})")
                }
            } else {
                format!("new {prefix}{class}({call_args_str})")
            };

            (format!("({args_str})"), body)
        }
        ImportFunctionKind::Method {
            class,
            kind: MethodKind::Operation(operation),
            ..
        } if operation.is_static => {
            let class_object = format!("{prefix}{class}");
            match &operation.kind {
                // A static getter/setter accesses the property on the class object
                // itself (e.g. `Number.NAN`) rather than calling a static method.
                OperationKind::Getter(property) => {
                    let property = property
                        .as_deref()
                        .unwrap_or_else(|| func.function.infer_getter_property());
                    (
                        "()".to_string(),
                        js_property_access(&class_object, property),
                    )
                }
                OperationKind::Setter(property) => {
                    let property = property
                        .clone()
                        .or_else(|| func.function.infer_setter_property().ok())
                        .unwrap_or_else(|| "value".to_string());
                    (
                        "(value)".to_string(),
                        format!("{} = value", js_property_access(&class_object, &property)),
                    )
                }
                _ => {
                    let method = js_property_access(&class_object, js_name);
                    (
                        format!("({args_str})"),
                        format!("{method}({call_args_str})"),
                    )
                }
            }
        }
        ImportFunctionKind::Method {
            kind: MethodKind::Operation(operation),
            ..
        } => match &operation.kind {
            OperationKind::Regular | OperationKind::RegularThis => {
                let method = js_property_access("obj", js_name);
                if args.is_empty() {
                    ("(obj)".to_string(), format!("{method}()"))
                } else {
                    (
                        format!("(obj, {args_str})"),
                        format!("{method}({call_args_str})"),
                    )
                }
            }
            OperationKind::Getter(property) => (
                "(obj)".to_string(),
                js_property_access(
                    "obj",
                    property
                        .as_deref()
                        .unwrap_or_else(|| func.function.infer_getter_property()),
                ),
            ),
            OperationKind::Setter(property) => {
                let property = property
                    .clone()
                    .or_else(|| func.function.infer_setter_property().ok())
                    .unwrap_or_else(|| "value".to_string());
                (
                    "(obj, value)".to_string(),
                    format!("{} = value", js_property_access("obj", &property)),
                )
            }
            OperationKind::IndexingGetter => ("(obj, index)".to_string(), "obj[index]".to_string()),
            OperationKind::IndexingSetter => (
                "(obj, index, value)".to_string(),
                "obj[index] = value".to_string(),
            ),
            OperationKind::IndexingDeleter => {
                ("(obj, index)".to_string(), "delete obj[index]".to_string())
            }
        },
    };

    let body = if func.catch && !skip_catch_wrapper {
        wrap_body_with_try_catch(&body)
    } else {
        body
    };

    match hoisted_callee {
        Some(callee) => {
            // `{{`/`}}` escape literal braces: this string is itself a Rust
            // format template (the `{__wry_module}` placeholder is substituted
            // when the binding is rendered).
            format!("(() => {{{{ const __wry_callee = {callee}; return {params} => {body}; }}}})()")
        }
        None => format!("{params} => {body}"),
    }
}

fn js_property_access(object: &str, property: &str) -> String {
    format!("{object}[{}]", js_string_literal(property))
}

/// Build a JS access prefix by appending the dotted namespace (if any) to `prefix`.
pub(super) fn namespace_prefix(prefix: &str, namespace: Option<&[String]>) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{prefix}{}.", ns.join(".")),
        _ => prefix.to_string(),
    }
}

fn js_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for ch in value.chars() {
        match ch {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            '\u{08}' => literal.push_str("\\b"),
            '\u{0c}' => literal.push_str("\\f"),
            ch if ch < ' ' => {
                use core::fmt::Write;
                write!(&mut literal, "\\u{:04x}", ch as u32).unwrap();
            }
            ch => literal.push(ch),
        }
    }
    literal.push('"');
    literal
}

fn wrap_body_with_try_catch(body: &str) -> String {
    format!(
        "{{{{ try {{{{ return {{{{ ok: {body} }}}}; }}}} catch(e) {{{{ return {{{{ err: e }}}}; }}}} }}}}"
    )
}

pub(super) fn mark_async_promise_handled_js_code(js_code: &str) -> String {
    let Some((params, body)) = js_code.split_once(" => ") else {
        panic!("generated async JS code should be an arrow function");
    };
    format!(
        "{params} => {{{{ const __wryPromise = Promise.resolve({body}); __wryPromise.then(undefined, () => undefined); return __wryPromise; }}}}"
    )
}
