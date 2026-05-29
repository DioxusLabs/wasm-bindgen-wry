use crate::ast::{ImportFunction, ImportFunctionKind};

fn generate_vendor_prefixed_constructor(class: &str, prefixes: &[String], prefix: &str) -> String {
    // Start with the base class name (no prefix)
    let mut result = format!("(typeof {prefix}{class} !== 'undefined' ? {prefix}{class} : ");

    // Add each vendor prefix
    for (i, vendor_prefix) in prefixes.iter().enumerate() {
        let prefixed_class = format!("{vendor_prefix}{class}");
        if i == prefixes.len() - 1 {
            // Last one - end with undefined if none found
            result.push_str(&format!(
                "(typeof {prefix}{prefixed_class} !== 'undefined' ? {prefix}{prefixed_class} : undefined)"
            ));
        } else {
            result.push_str(&format!(
                "(typeof {prefix}{prefixed_class} !== 'undefined' ? {prefix}{prefixed_class} : "
            ));
        }
    }

    // Close all the parentheses
    result.push(')');
    result
}

/// Generate JavaScript code for the function
pub(super) fn generate_js_code(
    func: &ImportFunction,
    vendor_prefixes: &std::collections::HashMap<String, Vec<String>>,
    prefix: &str,
    skip_catch_wrapper: bool,
) -> String {
    let js_name = &func.js_name;

    let prefix = namespace_prefix(prefix, func.js_namespace.as_deref());

    // Use a{index} naming to avoid conflicts with JS reserved words
    let args: Vec<_> = (0..func.arguments.len()).map(|i| format!("a{i}")).collect();
    let args_str = args.join(", ");

    let (params, body) = match &func.kind {
        ImportFunctionKind::Normal => {
            let callee = if prefix.is_empty() {
                js_name.to_string()
            } else {
                let object = prefix.trim_end_matches('.');
                js_property_access(object, js_name)
            };
            (format!("({args_str})"), format!("{callee}({args_str})"))
        }
        ImportFunctionKind::Method { .. } => {
            let method = js_property_access("obj", js_name);
            if args.is_empty() {
                ("(obj)".to_string(), format!("{method}()"))
            } else {
                (
                    format!("(obj, {args_str})"),
                    format!("{method}({args_str})"),
                )
            }
        }
        ImportFunctionKind::Getter { property, .. } => {
            ("(obj)".to_string(), js_property_access("obj", property))
        }
        ImportFunctionKind::Setter { property, .. } => (
            "(obj, value)".to_string(),
            format!("{} = value", js_property_access("obj", property)),
        ),
        ImportFunctionKind::IndexingGetter { .. } => {
            // obj[index] - takes one argument (the index)
            ("(obj, index)".to_string(), "obj[index]".to_string())
        }
        ImportFunctionKind::IndexingSetter { .. } => {
            // obj[index] = value - takes two arguments (index and value)
            (
                "(obj, index, value)".to_string(),
                "obj[index] = value".to_string(),
            )
        }
        ImportFunctionKind::IndexingDeleter { .. } => {
            // delete obj[index] - takes one argument (the index)
            ("(obj, index)".to_string(), "delete obj[index]".to_string())
        }
        ImportFunctionKind::Constructor { class } => {
            // Check if this type has vendor prefixes
            let body = if let Some(prefixes) = vendor_prefixes.get(class) {
                if !prefixes.is_empty() {
                    // Generate vendor-prefixed fallback code
                    let constructor_expr =
                        generate_vendor_prefixed_constructor(class, prefixes, &prefix);
                    format!("new ({constructor_expr})({args_str})")
                } else {
                    format!("new {prefix}{class}({args_str})")
                }
            } else {
                format!("new {prefix}{class}({args_str})")
            };

            (format!("({args_str})"), body)
        }
        ImportFunctionKind::StaticMethod { class } => {
            let class_object = format!("{prefix}{class}");
            let method = js_property_access(&class_object, js_name);
            (format!("({args_str})"), format!("{method}({args_str})"))
        }
    };

    // Wrap in try-catch if catch attribute is present
    // Skip for async functions since the Promise adapter already returns
    // Result<JsValue, JsValue>.
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

/// Wrap JavaScript body in try-catch block for error handling
fn wrap_body_with_try_catch(body: &str) -> String {
    // Wrap the body in try-catch and return Result-like object
    format!(
        "{{{{ try {{{{ return {{{{ ok: {body} }}}}; }}}} catch(e) {{{{ return {{{{ err: e }}}}; }}}} }}}}"
    )
}

/// Wrap an async import's arrow function so the promise it returns carries a
/// rejection handler from the moment it is created.
///
/// `JsFuture` attaches the real resolve/reject handlers once Rust awaits the
/// promise, but that attach is a separate Rust-to-JS call. In nonbatched mode
/// the promise is created and handed back to Rust in one round-trip, so an
/// already-rejected promise reaches the event loop's unhandled-rejection check
/// before the real handler arrives. Attaching a no-op rejection handler in the
/// same JS turn marks the promise handled; `JsFuture`'s later handlers still
/// receive the settled value.
pub(super) fn async_promise_guard_js_code(js_code: &str) -> String {
    let Some((params, body)) = js_code.split_once(" => ") else {
        panic!("generated async JS code should be an arrow function");
    };
    format!(
        "{params} => {{{{ const __wryPromise = Promise.resolve({body}); __wryPromise.then(undefined, () => undefined); return __wryPromise; }}}}"
    )
}
