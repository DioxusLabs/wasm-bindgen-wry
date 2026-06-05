//! Runtime-side JS registry generation.

use crate::wire::{
    JsClassMemberKind, JsClassMemberSpec, JsClassSpec, JsFunctionSpec, JsReexportSpec,
    ObjectHandle, TypeDef,
};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use once_cell::sync::Lazy;

/// Registry of JS functions collected via inventory.
pub(crate) struct FunctionRegistry {
    functions: String,
    function_specs: Vec<JsFunctionSpec>,
    modules: BTreeMap<String, &'static str>,
}

pub(crate) static FUNCTION_REGISTRY: Lazy<FunctionRegistry> =
    Lazy::new(FunctionRegistry::collect_from_inventory);

fn generate_args(count: usize) -> String {
    (0..count)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn object_handle_type_def() -> TypeDef {
    TypeDef::of::<ObjectHandle>()
}

pub(crate) fn type_def_js_array_literal(def: &TypeDef) -> String {
    let mut out = String::from("[");
    for (index, byte) in def.bytes().iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&byte.to_string());
    }
    out.push(']');
    out
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
                write!(&mut literal, "\\u{:04x}", ch as u32).unwrap();
            }
            ch => literal.push(ch),
        }
    }
    literal.push('"');
    literal
}

fn js_type_defs_literal(types: &[TypeDef]) -> String {
    let mut out = String::from("[");
    for (i, ty) in types.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&type_def_js_array_literal(ty));
    }
    out.push(']');
    out
}

fn js_optional_type_def_literal(ty: Option<TypeDef>) -> String {
    ty.map(|ty| type_def_js_array_literal(&ty))
        .unwrap_or_else(|| "null".to_string())
}

fn js_string_array_literal(values: &[&str]) -> String {
    let mut out = String::from("[");
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&js_string_literal(value));
    }
    out.push(']');
    out
}

fn install_window_path_statement(namespace: &[&str], name: &str, value_expr: &str) -> String {
    format!(
        "__wryInstallPath({}, {}, {value_expr});",
        js_string_array_literal(namespace),
        js_string_literal(name),
    )
}

fn window_path_expression(namespace: &[&str], name: &str) -> String {
    let mut expr = String::from("window");
    for segment in namespace {
        expr.push('[');
        expr.push_str(&js_string_literal(segment));
        expr.push(']');
    }
    expr.push('[');
    expr.push_str(&js_string_literal(name));
    expr.push(']');
    expr
}

struct ClassSpecParts {
    class_name: &'static str,
    js_name: &'static str,
    js_namespace: &'static [&'static str],
    private: bool,
    extends: Option<&'static str>,
    extends_js_class: Option<&'static str>,
    extends_js_namespace: &'static [&'static str],
}

fn class_spec_parts(class_spec: &JsClassSpec) -> ClassSpecParts {
    let (
        class_name,
        js_name,
        js_namespace,
        private,
        extends,
        extends_js_class,
        extends_js_namespace,
    ) = class_spec.parts();
    ClassSpecParts {
        class_name,
        js_name,
        js_namespace,
        private,
        extends,
        extends_js_class,
        extends_js_namespace,
    }
}

struct ClassMemberParts {
    class_name: &'static str,
    member_name: &'static str,
    export_name: &'static str,
    arg_count: usize,
    arg_types: Vec<TypeDef>,
    return_type: Option<TypeDef>,
    kind: JsClassMemberKind,
}

fn class_member_parts(member: &JsClassMemberSpec) -> ClassMemberParts {
    let (class_name, member_name, export_name, arg_count, arg_types, return_type, kind) =
        member.parts();
    ClassMemberParts {
        class_name,
        member_name,
        export_name,
        arg_count,
        arg_types,
        return_type,
        kind,
    }
}

fn call_export_expression(
    export_name: &str,
    arg_types: &[TypeDef],
    return_type: Option<TypeDef>,
    args_call: &str,
) -> String {
    format!(
        r#"window.__wryCallExport("{}", {}, {}, [{}])"#,
        export_name,
        js_type_defs_literal(arg_types),
        js_optional_type_def_literal(return_type),
        args_call,
    )
}

impl FunctionRegistry {
    fn collect_from_inventory() -> Self {
        let specs: Vec<_> = inventory::iter::<JsFunctionSpec>().copied().collect();
        let mut modules = BTreeMap::new();

        for spec in &specs {
            let Some(module) = spec.module() else {
                continue;
            };
            let hash = format!("{:x}", module.const_hash());
            if let Some(content) = module.content() {
                let module_path = format!("{hash}.js");
                modules.entry(module_path).or_insert(content);
            }
        }
        let reexports: Vec<_> = inventory::iter::<JsReexportSpec>().copied().collect();
        for spec in &reexports {
            let Some(module) = spec.module() else {
                continue;
            };
            let hash = format!("{:x}", module.const_hash());
            if let Some(content) = module.content() {
                let module_path = format!("{hash}.js");
                modules.entry(module_path).or_insert(content);
            }
        }

        let mut script = String::new();
        script.push_str("(async () => {\n");
        script.push_str(
            "  function __wryInstallPath(namespace, name, value) {\n\
                let target = window;\n\
                for (const segment of namespace) {\n\
                  target = target[segment] ||= {};\n\
                }\n\
                target[name] = value;\n\
              }\n",
        );
        script.push_str("  window.__wryClassRegistry ||= {};\n");

        let mut imported_modules = alloc::collections::BTreeSet::new();
        for spec in &specs {
            let Some(module) = spec.module() else {
                continue;
            };
            let hash = format!("{:x}", module.const_hash());
            if imported_modules.insert(hash.clone()) {
                if let Some(specifier) = module.raw_specifier() {
                    let specifier = js_string_literal(specifier);
                    writeln!(
                        &mut script,
                        "  const module_{hash} = await import({specifier});"
                    )
                    .unwrap();
                } else {
                    writeln!(
                        &mut script,
                        "  const module_{hash} = await import('/__wbg__/snippets/{hash}.js');"
                    )
                    .unwrap();
                }
            }
        }
        for spec in &reexports {
            let Some(module) = spec.module() else {
                continue;
            };
            let hash = format!("{:x}", module.const_hash());
            if imported_modules.insert(hash.clone()) {
                if let Some(specifier) = module.raw_specifier() {
                    let specifier = js_string_literal(specifier);
                    writeln!(
                        &mut script,
                        "  const module_{hash} = await import({specifier});"
                    )
                    .unwrap();
                } else {
                    writeln!(
                        &mut script,
                        "  const module_{hash} = await import('/__wbg__/snippets/{hash}.js');"
                    )
                    .unwrap();
                }
            }
        }

        script.push_str("  window.setFunctionRegistry([");
        for (i, spec) in specs.iter().enumerate() {
            if i > 0 {
                script.push_str(",\n");
            }
            let js_code = spec.render_js_code();
            write!(&mut script, "{js_code}").unwrap();
        }
        script.push_str("]);\n");

        for reexport in &reexports {
            let (name, namespace, value_expr) = reexport.parts();
            writeln!(
                &mut script,
                "  {}",
                install_window_path_statement(namespace, name, &value_expr)
            )
            .unwrap();
        }

        let mut class_members: BTreeMap<&str, Vec<ClassMemberParts>> = BTreeMap::new();
        for member in inventory::iter::<JsClassMemberSpec>() {
            let member = class_member_parts(member);
            class_members
                .entry(member.class_name)
                .or_default()
                .push(member);
        }

        let mut class_specs: BTreeMap<&str, ClassSpecParts> = BTreeMap::new();
        for class_spec in inventory::iter::<JsClassSpec>() {
            let class_spec = class_spec_parts(class_spec);
            class_specs
                .entry(class_spec.class_name)
                .or_insert(class_spec);
        }

        let mut class_names: alloc::collections::BTreeSet<&str> =
            alloc::collections::BTreeSet::new();
        class_names.extend(class_specs.keys().copied());
        class_names.extend(class_members.keys().copied());

        let mut pending: Vec<&str> = class_names.into_iter().collect();
        let mut ordered_classes = Vec::new();
        let mut emitted_classes = alloc::collections::BTreeSet::new();
        while !pending.is_empty() {
            let before = pending.len();
            let mut i = 0;
            while i < pending.len() {
                let class_name = pending[i];
                let parent = class_specs.get(class_name).and_then(|spec| spec.extends);
                let parent_ready = parent.is_none_or(|parent| {
                    !class_specs.contains_key(parent) || emitted_classes.contains(parent)
                });
                if parent_ready {
                    ordered_classes.push(class_name);
                    emitted_classes.insert(class_name);
                    pending.remove(i);
                } else {
                    i += 1;
                }
            }
            if pending.len() == before {
                ordered_classes.append(&mut pending);
            }
        }

        for class_name in ordered_classes {
            let members = class_members
                .get(class_name)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let class_spec = class_specs.get(class_name);
            let extends_expr = class_spec
                .and_then(|spec| {
                    spec.extends_js_class
                        .map(|parent| window_path_expression(spec.extends_js_namespace, parent))
                        .or_else(|| {
                            spec.extends.map(|parent| {
                                format!("window.__wryClassRegistry[{}]", js_string_literal(parent))
                            })
                        })
                })
                .map(|parent_expr| format!(" extends {parent_expr}"))
                .unwrap_or_default();
            let drop_export_name = format!("{class_name}::__drop");
            let drop_arg_types = [object_handle_type_def()];
            let drop_call =
                call_export_expression(&drop_export_name, &drop_arg_types, None, "handle");
            let constructor_body = members
                .iter()
                .find(|member| matches!(member.kind, JsClassMemberKind::Constructor))
                .map(|member| {
                    let args = generate_args(member.arg_count);
                    let args_call = if member.arg_count > 0 { &args } else { "" };
                    let call = call_export_expression(
                        member.export_name,
                        &member.arg_types,
                        member.return_type.clone(),
                        args_call,
                    );
                    format!(
                        "    constructor({args}) {{\n      const value = {call};\n      if (value && typeof value.then === \"function\") {{\n        return value.then((resolved) => typeof resolved === \"number\" ? {class_name}.__wrap(resolved) : resolved);\n      }}\n      return {class_name}.__wrap(value);\n    }}"
                    )
                })
                .unwrap_or_else(|| {
                    format!(
                        r#"    constructor(handle) {{
      this.__handle = handle;
      this.__className = "{class_name}";
      window.__wryExportRegistry.register(this, {{ handle, className: "{class_name}" }});
    }}"#
                    )
                });
            writeln!(
                &mut script,
                r#"  class {class_name}{extends_expr} {{
{constructor_body}
    static __wrap(handle) {{
      const obj = Object.create({class_name}.prototype);
      obj.__handle = handle;
      obj.__className = "{class_name}";
      window.__wryExportRegistry.register(obj, {{ handle, className: "{class_name}" }});
      return obj;
    }}
    free() {{
      const handle = this.__handle;
      this.__handle = 0;
      if (handle !== 0) {drop_call};
    }}"#
            )
            .unwrap();

            let mut getters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();
            let mut setters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();

            for member in members {
                match member.kind {
                    JsClassMemberKind::Method => {
                        let args = generate_args(member.arg_count);
                        let args_with_handle = if member.arg_count > 0 {
                            format!("this.__handle, {args}")
                        } else {
                            "this.__handle".to_string()
                        };
                        let mut arg_types = vec![object_handle_type_def()];
                        arg_types.extend(member.arg_types.iter().cloned());
                        let call = call_export_expression(
                            member.export_name,
                            &arg_types,
                            member.return_type.clone(),
                            &args_with_handle,
                        );
                        writeln!(
                            &mut script,
                            r#"    {}({}) {{ return {}; }}"#,
                            member.member_name, args, call
                        )
                        .unwrap();
                    }
                    JsClassMemberKind::Getter => {
                        getters.insert(member.member_name, member);
                    }
                    JsClassMemberKind::Setter => {
                        setters.insert(member.member_name, member);
                    }
                    _ => {}
                }
            }

            let mut property_names: alloc::collections::BTreeSet<&str> =
                alloc::collections::BTreeSet::new();
            property_names.extend(getters.keys());
            property_names.extend(setters.keys());

            let accessor_call = |member: &ClassMemberParts, args_call: &str| {
                let mut arg_types = vec![object_handle_type_def()];
                arg_types.extend(member.arg_types.iter().cloned());
                call_export_expression(
                    member.export_name,
                    &arg_types,
                    member.return_type.clone(),
                    args_call,
                )
            };

            for prop_name in property_names {
                if let Some(g) = getters.get(prop_name) {
                    let call = accessor_call(g, "this.__handle");
                    writeln!(&mut script, r#"    get {prop_name}() {{ return {call}; }}"#).unwrap();
                }
                if let Some(s) = setters.get(prop_name) {
                    let call = accessor_call(s, "this.__handle, v");
                    writeln!(&mut script, r#"    set {prop_name}(v) {{ {call}; }}"#).unwrap();
                }
            }

            script.push_str("  }\n");

            for member in members {
                let is_constructor = match member.kind {
                    JsClassMemberKind::Constructor => true,
                    JsClassMemberKind::StaticMethod => false,
                    _ => continue,
                };
                let args = generate_args(member.arg_count);
                let args_call = if member.arg_count > 0 { &args } else { "" };
                let call = call_export_expression(
                    member.export_name,
                    &member.arg_types,
                    member.return_type.clone(),
                    args_call,
                );
                let method_name = member.member_name;
                let body = if is_constructor {
                    format!(
                        "const value = {call}; if (value && typeof value.then === \"function\") {{ return value.then((resolved) => typeof resolved === \"number\" ? {class_name}.__wrap(resolved) : resolved); }} return {class_name}.__wrap(value);"
                    )
                } else {
                    format!("return {call};")
                };
                writeln!(
                    &mut script,
                    r#"  {class_name}.{method_name} = function({args}) {{ {body} }};"#
                )
                .unwrap();
            }

            writeln!(
                &mut script,
                "  window.__wryClassRegistry[{}] = {class_name};",
                js_string_literal(class_name)
            )
            .unwrap();
            match class_spec {
                Some(spec) if !spec.private => {
                    writeln!(
                        &mut script,
                        "  {}",
                        install_window_path_statement(spec.js_namespace, spec.js_name, class_name)
                    )
                    .unwrap();
                }
                None => {
                    writeln!(
                        &mut script,
                        "  {}",
                        install_window_path_statement(&[], class_name, class_name)
                    )
                    .unwrap();
                }
                _ => {}
            }
        }

        let mut start_calls = Vec::new();
        for export in inventory::iter::<crate::wire::JsFreeExportSpec>() {
            let (name, namespace, arg_count, arg_names, arg_types, return_type, this, public, start) =
                export.parts();
            let args = if arg_names.is_empty() {
                generate_args(arg_count)
            } else {
                arg_names.join(", ")
            };
            let args_call = if this {
                if args.is_empty() {
                    "this".to_string()
                } else {
                    format!("this, {args}")
                }
            } else {
                args.clone()
            };
            let call = call_export_expression(name, &arg_types, return_type.clone(), &args_call);
            if public {
                let wrapper = format!("function({args}) {{ return {call}; }}");
                writeln!(
                    &mut script,
                    "  {}",
                    install_window_path_statement(namespace, name, &wrapper)
                )
                .unwrap();
            }
            if start {
                start_calls.push(call);
            }
        }

        if start_calls.is_empty() {
            script
                .push_str("  await fetch(`/__wbg__/initialized`, { method: 'POST', body: [] });\n");
        } else {
            script.push_str(
                "  await fetch(`/__wbg__/preinitialized`, { method: 'POST', body: [] });\n",
            );
            for call in start_calls {
                writeln!(&mut script, "  await {call};").unwrap();
            }
            script
                .push_str("  await fetch(`/__wbg__/initialized`, { method: 'POST', body: [] });\n");
        }
        script.push_str("})();\n");

        Self {
            functions: script,
            function_specs: specs,
            modules,
        }
    }

    pub(crate) fn resolve_function(&self, spec: JsFunctionSpec) -> Option<u32> {
        self.function_specs
            .iter()
            .position(|s| s.identity_eq(&spec))
            .map(|index| index as u32)
    }

    pub(crate) fn script(&self) -> &str {
        &self.functions
    }

    pub(crate) fn get_module(&self, path: &str) -> Option<&'static str> {
        self.modules.get(path).copied()
    }
}
