//! Runtime-side JS registry generation.

use crate::wire::{
    JsClassMemberKind, JsClassMemberSpec, JsClassSpec, JsFunctionSpec, JsModuleSpec,
    JsReexportSpec, ObjectHandle, TypeDef,
};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use once_cell::sync::Lazy;

/// Modules registered at runtime by `link_to!`. Unlike the inventory-collected
/// modules in [`FUNCTION_REGISTRY`], a `link_to!` module's content is only known
/// once the macro-expanded call runs, so it is stored here keyed by the same
/// content-hash path (`{hash:x}.js`) the snippet protocol serves.
static LINKED_MODULES: Lazy<std::sync::Mutex<BTreeMap<String, &'static str>>> =
    Lazy::new(|| std::sync::Mutex::new(BTreeMap::new()));

/// Register a `link_to!(module = ...)` / `link_to!(inline_js = ...)` snippet and
/// return the URL the WebView fetches it from. The content is hashed exactly as
/// the snippet protocol expects, so the returned `/__wbg__/snippets/{hash}.js`
/// resolves to `content` via [`linked_module`].
pub fn register_linked_module(content: &'static str) -> String {
    let hash = JsModuleSpec::new(content).const_hash();
    let path = format!("{hash:x}.js");
    LINKED_MODULES.lock().unwrap().insert(path.clone(), content);
    format!("/__wbg__/snippets/{path}")
}

/// Resolve a `link_to!(raw_module = ...)` specifier. A raw module is an opaque
/// specifier the host is expected to resolve, so it is returned verbatim; the
/// snippet protocol does not serve it, so a fetch of an unknown specifier fails.
pub fn link_to_raw_specifier(spec: &str) -> String {
    spec.into()
}

/// The content of a `link_to!`-registered module for the given snippet path
/// (`{hash:x}.js`), if one was registered.
pub(crate) fn linked_module(path: &str) -> Option<&'static str> {
    LINKED_MODULES.lock().unwrap().get(path).copied()
}

/// Registry of JS functions collected via inventory.
pub(crate) struct FunctionRegistry {
    functions: String,
    function_specs: Vec<JsFunctionSpec>,
    modules: BTreeMap<String, &'static str>,
}

pub(crate) static FUNCTION_REGISTRY: Lazy<FunctionRegistry> =
    Lazy::new(FunctionRegistry::collect_from_inventory);

fn generate_args(arg_types: &[TypeDef]) -> String {
    (0..arg_types.len())
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn object_handle_type_def() -> TypeDef {
    TypeDef::of::<ObjectHandle>()
}

fn unit_type_def() -> TypeDef {
    TypeDef::of::<()>()
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

/// The namespace-qualified registry key for a class, matching
/// `wasm_bindgen_shared::qualified_name` and the macro's `qualified_class_name`
/// (`ns1__ns2__Name`, else `Name`).
fn qualified_class_name(namespace: &[&str], name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}__{}", namespace.join("__"), name)
    }
}

struct ClassSpecParts {
    class_name: &'static str,
    js_name: &'static str,
    js_namespace: &'static [&'static str],
    private: bool,
    extends: Option<&'static str>,
    extends_js_class: Option<&'static str>,
    extends_js_namespace: &'static [&'static str],
    inspectable: bool,
    public_fields: &'static [&'static str],
}

impl ClassSpecParts {
    /// The registry key of this class's direct parent, if it `extends` one. Uses
    /// the parent's JS identity (`extends_js_class` + `extends_js_namespace`) when
    /// declared (the parent was renamed via `js_name`), else the `extends` Rust
    /// last segment (which equals the parent's key in the no-rename case).
    fn parent_class_name(&self) -> Option<String> {
        self.extends.map(|extends| match self.extends_js_class {
            Some(js_class) => qualified_class_name(self.extends_js_namespace, js_class),
            None => extends.to_string(),
        })
    }
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
        inspectable,
        public_fields,
    ) = class_spec.parts();
    ClassSpecParts {
        class_name,
        js_name,
        js_namespace,
        private,
        extends,
        extends_js_class,
        extends_js_namespace,
        inspectable,
        public_fields,
    }
}

struct ClassMemberParts {
    class_name: &'static str,
    member_name: &'static str,
    export_name: &'static str,
    arg_types: Vec<TypeDef>,
    return_type: TypeDef,
    kind: JsClassMemberKind,
    consumes_self: bool,
}

fn class_member_parts(member: &JsClassMemberSpec) -> ClassMemberParts {
    let (class_name, member_name, export_name, arg_types, return_type, kind, consumes_self) =
        member.parts();
    ClassMemberParts {
        class_name,
        member_name,
        export_name,
        arg_types,
        return_type,
        kind,
        consumes_self,
    }
}

fn call_export_expression(
    export_name: &str,
    arg_types: &[TypeDef],
    return_type: &TypeDef,
    args_call: &str,
) -> String {
    format!(
        r#"window.__wryCallExport("{}", {}, {}, [{}])"#,
        export_name,
        js_type_defs_literal(arg_types),
        type_def_js_array_literal(return_type),
        args_call,
    )
}

/// The `const __h = ...;` receiver-handle read and the `consume` tail for an
/// instance member defined on `class_name`. The read resolves the member's
/// defining-class slot (an ancestor view when the member is inherited by a
/// descendant). A consuming (`self`-by-value) member gates against subclass
/// prototype dispatch and zeroes every per-class slot so the receiver is dead
/// afterward, mirroring wasm-bindgen's `__destroy_into_raw`.
fn handle_read_and_consume(
    class_name: &str,
    consumes_self: bool,
    chain: &[String],
) -> (String, String) {
    if consumes_self {
        let read = format!("const __h = __wryConsumeHandle(this, \"{class_name}\");");
        let mut consume = String::from(" this.__handle = 0;");
        consume.push_str(&format!(" this.__wbg_ptr_{class_name} = 0;"));
        for ancestor in chain {
            consume.push_str(&format!(" this.__wbg_ptr_{ancestor} = 0;"));
        }
        (read, consume)
    } else {
        (
            format!("const __h = __wryClassHandle(this, \"{class_name}\");"),
            String::new(),
        )
    }
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
        // Read the per-class handle slot for a member defined on `className`. A
        // member inherited by a descendant reads the descendant's ancestor-view
        // slot, so the export operates on the ancestor's shared data, not the
        // descendant's own. The own `__handle` is the moved-value sentinel: a
        // by-value pass zeroes it, so check it first (the per-class slots are only
        // zeroed by the receiver's own `free`/consume).
        script.push_str(
            "  function __wryClassHandle(obj, className) {\n\
                if (obj.__handle === 0) {\n\
                  throw new Error(\"Attempt to use a moved value\");\n\
                }\n\
                const slot = obj[\"__wbg_ptr_\" + className];\n\
                return typeof slot === \"number\" ? slot : obj.__handle;\n\
              }\n",
        );
        // Gate a `self`-by-value (consuming) member dispatched on a wasm-bindgen
        // descendant via the prototype chain: handing the descendant's pointer to
        // an ancestor's consuming shim is type confusion. The own handle differs
        // from the defining class's slot only for such descendant dispatch (a real
        // instance or a JS-only subclass keeps them equal), so reject then.
        script.push_str(
            "  function __wryConsumeHandle(obj, className) {\n\
                const handle = __wryClassHandle(obj, className);\n\
                if (obj.__handle !== handle) {\n\
                  throw new TypeError(className + \": cannot be invoked through subclass prototype dispatch\");\n\
                }\n\
                return handle;\n\
              }\n",
        );
        script.push_str("  window.__wryClassRegistry ||= {};\n");
        // Sentinel passed to `super(__wbgSuperSkip)` so a generated child
        // constructor short-circuits its parent's generated constructor: the Rust
        // child constructor already builds the inner parent value, so the JS
        // parent constructor must not allocate a second one.
        script.push_str(
            "  const __wbgSuperSkip = window.__wbgSuperSkip ||= Symbol('wry-bindgen.super-skip');\n",
        );

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

        // Each class's direct parent registry key (resolved through `js_name`
        // renames), plus the full ancestor chain (nearest first). The chain
        // drives per-class handle slots, ancestor-view upcasts, and gated drops.
        let direct_parent: BTreeMap<&str, String> = class_specs
            .iter()
            .filter_map(|(name, spec)| spec.parent_class_name().map(|parent| (*name, parent)))
            .collect();
        let ancestor_chain = |class_name: &str| -> Vec<String> {
            let mut chain = Vec::new();
            let mut current = class_name.to_string();
            while let Some(parent) = direct_parent.get(current.as_str()) {
                chain.push(parent.clone());
                current = parent.clone();
            }
            chain
        };

        let mut pending: Vec<&str> = class_names.into_iter().collect();
        let mut ordered_classes = Vec::new();
        let mut emitted_classes = alloc::collections::BTreeSet::new();
        while !pending.is_empty() {
            let before = pending.len();
            let mut i = 0;
            while i < pending.len() {
                let class_name = pending[i];
                let parent = direct_parent.get(class_name);
                let parent_ready = parent.is_none_or(|parent| {
                    !class_specs.contains_key(parent.as_str())
                        || emitted_classes.contains(parent.as_str())
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
            let chain = ancestor_chain(class_name);
            let has_parent = !chain.is_empty();
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

            // JS that, after `obj.__handle` (= the own handle) is set, populates
            // each per-class slot. The own slot equals the own handle; each
            // ancestor slot is a separate handle produced by chaining the
            // direct-parent upcast exports (each takes the previous link's
            // handle). Used by both the constructor (on `this`) and `__wrap`.
            let populate_slots = |target: &str| -> String {
                let mut out = String::new();
                writeln!(
                    &mut out,
                    "{target}.__wbg_ptr_{class_name} = {target}.__handle;"
                )
                .unwrap();
                let mut prev_handle = format!("{target}.__handle");
                let mut prev_class = class_name.to_string();
                for ancestor in &chain {
                    let upcast = format!("__upcast_{prev_class}");
                    let call = call_export_expression(
                        &upcast,
                        &[object_handle_type_def()],
                        &object_handle_type_def(),
                        &prev_handle,
                    );
                    writeln!(&mut out, "{target}.__wbg_ptr_{ancestor} = {call};").unwrap();
                    prev_handle = format!("{target}.__wbg_ptr_{ancestor}");
                    prev_class = ancestor.clone();
                }
                out
            };

            // Drop the own object plus every ancestor view (each holds a clone of
            // the shared parent cell, so dropping all decrements the shared Rc to
            // match the descendant's own reference).
            let drop_all = |handle_prefix: &str| -> String {
                let mut out = String::new();
                let own_drop = call_export_expression(
                    &drop_export_name,
                    &drop_arg_types,
                    &unit_type_def(),
                    &format!("{handle_prefix}_{class_name}"),
                );
                writeln!(
                    &mut out,
                    "if ({handle_prefix}_{class_name} !== 0) {own_drop};"
                )
                .unwrap();
                for ancestor in &chain {
                    let drop_call = call_export_expression(
                        &format!("{ancestor}::__drop"),
                        &drop_arg_types,
                        &unit_type_def(),
                        &format!("{handle_prefix}_{ancestor}"),
                    );
                    writeln!(
                        &mut out,
                        "if ({handle_prefix}_{ancestor} !== 0) {drop_call};"
                    )
                    .unwrap();
                }
                out
            };

            // The finalizer token lists `[className, handle]` for the own object
            // and each ancestor view (read off the wrapper after its slots are
            // populated) so a GC'd wrapper drops every backing object. The TS
            // FinalizationRegistry callback iterates this list.
            let finalizer_token = |target: &str| -> String {
                let mut entries = vec![format!(
                    "[\"{class_name}\", {target}.__wbg_ptr_{class_name}]"
                )];
                for ancestor in &chain {
                    entries.push(format!("[\"{ancestor}\", {target}.__wbg_ptr_{ancestor}]"));
                }
                format!("{{ drops: [{}] }}", entries.join(", "))
            };
            let super_call = if has_parent {
                "      super(__wbgSuperSkip);\n"
            } else {
                ""
            };

            let constructor_body = members
                .iter()
                .find(|member| matches!(member.kind, JsClassMemberKind::Constructor))
                .map(|member| {
                    let args = generate_args(&member.arg_types);
                    let call = call_export_expression(
                        member.export_name,
                        &member.arg_types,
                        &member.return_type,
                        &args,
                    );
                    let slots = populate_slots("this");
                    let token = finalizer_token("this");
                    // A child constructor runs `super(__wbgSuperSkip)` first so the
                    // parent's generated constructor short-circuits (no second
                    // parent allocation). When this constructor is itself reached
                    // via a child's `super(__wbgSuperSkip)`, bail immediately — the
                    // child sets up the real instance. The wrapper is its own
                    // unregister token so an explicit `free()` cancels the finalizer.
                    format!(
                        "    constructor({args}) {{\n{super_call}      if (arguments[0] === __wbgSuperSkip) return;\n      const value = {call};\n      if (value && typeof value.then === \"function\") {{\n        return value.then((resolved) => typeof resolved === \"number\" ? {class_name}.__wrap(resolved) : resolved);\n      }}\n      this.__handle = value;\n      this.__className = \"{class_name}\";\n{slots}      window.__wryExportRegistry.register(this, {token}, this);\n      return this;\n    }}"
                    )
                })
                .unwrap_or_else(|| {
                    // No exported constructor: `new ClassName()` throws, matching
                    // wasm-bindgen. Wrappers for instances created in Rust use
                    // `__wrap` (which bypasses the constructor via `Object.create`).
                    let body = if has_parent {
                        "    constructor() {\n      super(__wbgSuperSkip);\n      throw new Error(\"cannot invoke `new` directly\");\n    }"
                    } else {
                        "    constructor() {\n      throw new Error(\"cannot invoke `new` directly\");\n    }"
                    };
                    body.to_string()
                });
            let wrap_slots = populate_slots("obj");
            let wrap_token = finalizer_token("obj");
            // A `free()` on an already-freed/consumed wrapper (own handle zeroed)
            // hands a null pointer to Rust, which wasm-bindgen reports as
            // "null pointer passed to rust"; throw the same here.
            // `free()` dispatched via a subclass's prototype on a descendant would
            // feed the descendant's pointer to an ancestor's drop. Reject when the
            // own handle differs from this class's slot (true only for descendant
            // dispatch; a real instance or a JS-only subclass has them equal).
            let free_gate = format!(
                "      if (this.__handle === 0) {{ throw new Error(\"null pointer passed to rust\"); }}\n      if (this.__handle !== this.__wbg_ptr_{class_name}) {{ throw new TypeError(\"{class_name}: free cannot be invoked through subclass prototype dispatch\"); }}\n"
            );
            let free_drop = drop_all("handle");
            writeln!(
                &mut script,
                r#"  class {class_name}{extends_expr} {{
{constructor_body}
    static __wrap(handle) {{
      const obj = Object.create({class_name}.prototype);
      obj.__handle = handle;
      obj.__className = "{class_name}";
{wrap_slots}      window.__wryExportRegistry.register(obj, {wrap_token}, obj);
      return obj;
    }}
    free() {{
{free_gate}      const handle_{class_name} = this.__handle;
{ancestor_handle_reads}      this.__handle = 0;
{ancestor_handle_zeros}      window.__wryExportRegistry.unregister(this);
{free_drop}    }}"#,
                ancestor_handle_reads = chain
                    .iter()
                    .map(|a| format!("      const handle_{a} = this.__wbg_ptr_{a};\n"))
                    .collect::<String>(),
                ancestor_handle_zeros = {
                    let mut z = format!("      this.__wbg_ptr_{class_name} = 0;\n");
                    for a in &chain {
                        z.push_str(&format!("      this.__wbg_ptr_{a} = 0;\n"));
                    }
                    z
                },
            )
            .unwrap();

            let mut getters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();
            let mut setters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();
            let mut static_getters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();
            let mut static_setters: BTreeMap<&str, &ClassMemberParts> = BTreeMap::new();

            for member in members {
                match member.kind {
                    JsClassMemberKind::Method => {
                        let args = generate_args(&member.arg_types);
                        // `__h` is this defining class's per-class handle slot
                        // (its own handle for a direct instance, an ancestor view
                        // when inherited by a descendant). A consuming method also
                        // gates against subclass prototype dispatch.
                        let args_with_handle = if args.is_empty() {
                            "__h".to_string()
                        } else {
                            format!("__h, {args}")
                        };
                        let mut arg_types = vec![object_handle_type_def()];
                        arg_types.extend(member.arg_types.iter().cloned());
                        let call = call_export_expression(
                            member.export_name,
                            &arg_types,
                            &member.return_type,
                            &args_with_handle,
                        );
                        let (read, consume) =
                            handle_read_and_consume(class_name, member.consumes_self, &chain);
                        writeln!(
                            &mut script,
                            r#"    {}({}) {{ {read}{consume} return {}; }}"#,
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
                    JsClassMemberKind::StaticGetter => {
                        static_getters.insert(member.member_name, member);
                    }
                    JsClassMemberKind::StaticSetter => {
                        static_setters.insert(member.member_name, member);
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
                    &member.return_type,
                    args_call,
                )
            };

            for prop_name in property_names {
                if let Some(g) = getters.get(prop_name) {
                    let call = accessor_call(g, "__h");
                    let (read, consume) =
                        handle_read_and_consume(class_name, g.consumes_self, &chain);
                    writeln!(
                        &mut script,
                        r#"    get {prop_name}() {{ {read}{consume} return {call}; }}"#
                    )
                    .unwrap();
                }
                if let Some(s) = setters.get(prop_name) {
                    let call = accessor_call(s, "__h, v");
                    let (read, consume) =
                        handle_read_and_consume(class_name, s.consumes_self, &chain);
                    writeln!(
                        &mut script,
                        r#"    set {prop_name}(v) {{ {read}{consume} {call}; }}"#
                    )
                    .unwrap();
                } else if getters.contains_key(prop_name) {
                    // A getter-only (readonly) property still needs a setter or a
                    // strict-mode assignment throws; wasm-bindgen's non-strict
                    // wrappers silently ignore the write, so emit a no-op setter.
                    writeln!(&mut script, r#"    set {prop_name}(v) {{}}"#).unwrap();
                }
            }

            // Static property accessors take no receiver handle: they read/write
            // class-level state, so the call passes only the setter's value.
            let mut static_property_names: alloc::collections::BTreeSet<&str> =
                alloc::collections::BTreeSet::new();
            static_property_names.extend(static_getters.keys());
            static_property_names.extend(static_setters.keys());
            let static_accessor_call = |member: &ClassMemberParts, args_call: &str| {
                call_export_expression(
                    member.export_name,
                    &member.arg_types,
                    &member.return_type,
                    args_call,
                )
            };
            for prop_name in static_property_names {
                if let Some(g) = static_getters.get(prop_name) {
                    let call = static_accessor_call(g, "");
                    writeln!(
                        &mut script,
                        r#"    static get {prop_name}() {{ return {call}; }}"#
                    )
                    .unwrap();
                }
                if let Some(s) = static_setters.get(prop_name) {
                    let call = static_accessor_call(s, "v");
                    writeln!(
                        &mut script,
                        r#"    static set {prop_name}(v) {{ {call}; }}"#
                    )
                    .unwrap();
                }
            }

            // `#[wasm_bindgen(inspectable)]`: emit `toJSON` (an object built from
            // the public field getters) and `toString` (`JSON.stringify(this)`,
            // which calls `toJSON`). Skip whichever the user defines themselves so
            // an explicit `#[wasm_bindgen(js_name = toJSON/toString)]` method wins.
            if let Some(spec) = class_spec.filter(|spec| spec.inspectable) {
                let defines = |name: &str| members.iter().any(|m| m.member_name == name);
                if !defines("toJSON") {
                    let entries = spec
                        .public_fields
                        .iter()
                        .map(|f| format!("{f}: this.{f}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(&mut script, "    toJSON() {{ return {{ {entries} }}; }}").unwrap();
                }
                if !defines("toString") {
                    script.push_str("    toString() { return JSON.stringify(this); }\n");
                }
            }

            script.push_str("  }\n");

            for member in members {
                // Only `#[wasm_bindgen]` static methods become `ClassName.name`.
                // A `#[wasm_bindgen(constructor)]` (even one renamed away from
                // `new`) drives `new ClassName(..)` via the generated
                // `constructor()` above; it must NOT also leak as a static method.
                if !matches!(member.kind, JsClassMemberKind::StaticMethod) {
                    continue;
                }
                let args = generate_args(&member.arg_types);
                let call = call_export_expression(
                    member.export_name,
                    &member.arg_types,
                    &member.return_type,
                    &args,
                );
                let method_name = member.member_name;
                writeln!(
                    &mut script,
                    r#"  {class_name}.{method_name} = function({args}) {{ return {call}; }};"#
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
        for registration in inventory::iter::<crate::wire::JsExportSpecRegistration>() {
            let export = registration.spec();
            let signature = export.signature();
            let name = signature.name();
            let namespace = signature.namespace();
            let args_spec = signature.args();
            let arg_types: Vec<_> = args_spec.iter().map(|arg| arg.ty.clone()).collect();
            let return_type = signature.return_type().clone();
            let this = signature.this();
            let public = signature.public();
            let start = signature.start();
            let variadic = signature.variadic();
            let wire_arg_idents: Vec<String> = args_spec
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    if arg.name.is_empty() {
                        format!("a{i}")
                    } else {
                        arg.name.to_string()
                    }
                })
                .collect();
            let param_idents: Vec<String> = wire_arg_idents
                .iter()
                .skip(usize::from(this))
                .cloned()
                .collect();
            // A `#[wasm_bindgen(variadic)]` export collects every trailing
            // argument into its final parameter: the wrapper declares that
            // parameter as a JS rest parameter (`...aN`) so a spread call such as
            // `f(...arr)` arrives in Rust as one array, while the call passes the
            // already-gathered array straight through.
            let params = if variadic && !param_idents.is_empty() {
                let mut idents = param_idents.clone();
                let last = idents.pop().unwrap();
                idents.push(format!("...{last}"));
                idents.join(", ")
            } else {
                param_idents.join(", ")
            };
            let args = param_idents.join(", ");
            let args_call = if this {
                if args.is_empty() {
                    "this".to_string()
                } else {
                    format!("this, {args}")
                }
            } else {
                args.clone()
            };
            let call = call_export_expression(name, &arg_types, &return_type, &args_call);
            if public {
                // Name the wrapper after the export, so it appears in thrown
                // errors' stack traces (matching wasm-bindgen's named shims).
                let wrapper = format!("function {name}({params}) {{ return {call}; }}");
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
