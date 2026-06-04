//! Runtime-side JS registry generation.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use once_cell::sync::Lazy;
use crate::wire::{JsClassMemberKind, JsClassMemberSpec, JsFunctionSpec, ObjectHandle, TypeDef};

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
            let module_path = format!("{hash}.js");
            modules.entry(module_path).or_insert(module.content());
        }

        let mut script = String::new();
        script.push_str("(async () => {\n");

        let mut imported_modules = alloc::collections::BTreeSet::new();
        for spec in &specs {
            let Some(module) = spec.module() else {
                continue;
            };
            let hash = format!("{:x}", module.const_hash());
            if imported_modules.insert(hash.clone()) {
                writeln!(
                    &mut script,
                    "  const module_{hash} = await import('/__wbg__/snippets/{hash}.js');"
                )
                .unwrap();
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

        let mut class_members: BTreeMap<&str, Vec<ClassMemberParts>> = BTreeMap::new();
        for member in inventory::iter::<JsClassMemberSpec>() {
            let member = class_member_parts(member);
            class_members
                .entry(member.class_name)
                .or_default()
                .push(member);
        }

        for (class_name, members) in &class_members {
            let drop_export_name = format!("{class_name}::__drop");
            let drop_arg_types = [object_handle_type_def()];
            let drop_call =
                call_export_expression(&drop_export_name, &drop_arg_types, None, "handle");
            writeln!(
                &mut script,
                r#"  class {class_name} {{
    constructor(handle) {{
      this.__handle = handle;
      this.__className = "{class_name}";
      window.__wryExportRegistry.register(this, {{ handle, className: "{class_name}" }});
    }}
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
                    format!("const handle = {call}; return {class_name}.__wrap(handle);")
                } else {
                    format!("return {call};")
                };
                writeln!(
                    &mut script,
                    r#"  {class_name}.{method_name} = function({args}) {{ {body} }};"#
                )
                .unwrap();
            }

            writeln!(&mut script, "  window.{class_name} = {class_name};").unwrap();
        }

        script.push_str("  fetch(`/__wbg__/initialized`, { method: 'POST', body: [] });\n");
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
