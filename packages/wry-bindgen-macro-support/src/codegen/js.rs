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

    result.push(')');
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

    let (params, body) = match &func.kind {
        ImportFunctionKind::Normal => {
            let callee = if prefix.is_empty() {
                js_name.to_string()
            } else {
                let object = prefix.trim_end_matches('.');
                js_property_access(object, js_name)
            };
            (
                format!("({args_str})"),
                format!("{callee}({call_args_str})"),
            )
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
            let method = js_property_access(&class_object, js_name);
            (
                format!("({args_str})"),
                format!("{method}({call_args_str})"),
            )
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

    format!("{params} => {body}")
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

pub(super) fn async_promise_guard_js_code(js_code: &str) -> String {
    let Some((params, body)) = js_code.split_once(" => ") else {
        panic!("generated async JS code should be an arrow function");
    };
    format!(
        "{params} => {{{{ const __wryPromise = Promise.resolve({body}); __wryPromise.then(undefined, () => undefined); return __wryPromise; }}}}"
    )
}
