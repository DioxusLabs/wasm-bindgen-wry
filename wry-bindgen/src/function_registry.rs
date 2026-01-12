//! Function registry for collecting and managing JS functions and exports.
//!
//! This module provides the registry system that collects JS function specifications,
//! inline JS modules, and exported Rust types via the `inventory` crate.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use core::ops::Deref;
use once_cell::sync::{Lazy, OnceCell};

use crate::function::JSFunction;
use crate::ipc::{DecodedData, EncodedData};

/// Function specification for the registry
#[derive(Clone, Copy)]
pub struct JsFunctionSpec {
    /// Function that generates the JS code
    js_code: fn() -> String,
}

impl JsFunctionSpec {
    pub const fn new(js_code: fn() -> String) -> Self {
        Self { js_code }
    }

    /// Get the JS code generator function
    pub const fn js_code(&self) -> fn() -> String {
        self.js_code
    }

    pub const fn resolve_as<F>(&self) -> LazyJsFunction<F> {
        LazyJsFunction {
            spec: *self,
            inner: OnceCell::new(),
        }
    }
}

inventory::collect!(JsFunctionSpec);

/// A type that dynamically resolves to a JSFunction from the registry on first use.
pub struct LazyJsFunction<F> {
    spec: JsFunctionSpec,
    inner: OnceCell<JSFunction<F>>,
}

impl<F> Deref for LazyJsFunction<F> {
    type Target = JSFunction<F>;

    fn deref(&self) -> &Self::Target {
        self.inner.get_or_init(|| {
            FUNCTION_REGISTRY
                .get_function(self.spec)
                .unwrap_or_else(|| {
                    panic!("Function not found for code: {}", (self.spec.js_code())())
                })
        })
    }
}

/// Inline JS module info
#[derive(Clone, Copy)]
pub struct InlineJsModule {
    /// The JS module content
    content: &'static str,
}

impl InlineJsModule {
    pub const fn new(content: &'static str) -> Self {
        Self { content }
    }

    /// Get the JS module content
    pub const fn content(&self) -> &'static str {
        self.content
    }

    /// Calculate the hash of the module content for use as a filename
    /// This uses a simple FNV-1a hash that can also be computed at compile time
    pub fn hash(&self) -> String {
        format!("{:x}", self.const_hash())
    }

    /// Const-compatible hash function (FNV-1a)
    pub const fn const_hash(&self) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET_BASIS;
        let mut i = 0;
        let bytes = self.content.as_bytes();
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        hash
    }
}

inventory::collect!(InlineJsModule);

/// Type of class member for exported Rust structs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsClassMemberKind {
    /// Constructor function (e.g., `Counter.new`)
    Constructor,
    /// Instance method on prototype (e.g., `Counter.prototype.increment`)
    Method,
    /// Static method on class (e.g., `Counter.staticMethod`)
    StaticMethod,
    /// Property getter (e.g., `get count()`)
    Getter,
    /// Property setter (e.g., `set count(v)`)
    Setter,
}

/// Specification for a member of an exported Rust class
///
/// All class members (methods, constructors, getters, setters) are collected
/// and used to generate complete class code in FunctionRegistry.
#[derive(Clone, Copy)]
pub struct JsClassMemberSpec {
    /// The class name this member belongs to (e.g., "Counter")
    class_name: &'static str,
    /// The JavaScript member name (e.g., "increment", "count")
    member_name: &'static str,
    /// The export name for IPC calls (e.g., "Counter::increment")
    export_name: &'static str,
    /// Number of arguments (excluding self/handle)
    arg_count: usize,
    /// Type of member
    kind: JsClassMemberKind,
}

impl JsClassMemberSpec {
    pub const fn new(
        class_name: &'static str,
        member_name: &'static str,
        export_name: &'static str,
        arg_count: usize,
        kind: JsClassMemberKind,
    ) -> Self {
        Self {
            class_name,
            member_name,
            export_name,
            arg_count,
            kind,
        }
    }

    /// Get the class name this member belongs to
    pub const fn class_name(&self) -> &'static str {
        self.class_name
    }

    /// Get the JavaScript member name
    pub const fn member_name(&self) -> &'static str {
        self.member_name
    }

    /// Get the export name for IPC calls
    pub const fn export_name(&self) -> &'static str {
        self.export_name
    }

    /// Get the number of arguments (excluding self/handle)
    pub const fn arg_count(&self) -> usize {
        self.arg_count
    }

    /// Get the type of member
    pub const fn kind(&self) -> JsClassMemberKind {
        self.kind
    }
}

inventory::collect!(JsClassMemberSpec);

/// Specification for an exported Rust function/method callable from JavaScript.
///
/// This is used by the `#[wasm_bindgen]` macro when exporting structs and impl blocks.
/// Each export is registered via inventory and collected at runtime.
#[derive(Clone, Copy)]
pub struct JsExportSpec {
    /// The export name (e.g., "MyStruct::new", "MyStruct::method")
    pub name: &'static str,
    /// Handler function that decodes arguments, calls the Rust function, and encodes the result
    pub handler: fn(&mut DecodedData) -> Result<EncodedData, alloc::string::String>,
}

impl JsExportSpec {
    pub const fn new(
        name: &'static str,
        handler: fn(&mut DecodedData) -> Result<EncodedData, alloc::string::String>,
    ) -> Self {
        Self { name, handler }
    }
}

inventory::collect!(JsExportSpec);

/// Registry of JS functions collected via inventory
pub(crate) struct FunctionRegistry {
    functions: String,
    function_specs: Vec<JsFunctionSpec>,
    /// Map of module path -> module content for inline_js modules
    modules: BTreeMap<String, &'static str>,
}

/// The registry of javascript functions registered via inventory. This
/// is shared between all webviews.
pub(crate) static FUNCTION_REGISTRY: Lazy<FunctionRegistry> =
    Lazy::new(FunctionRegistry::collect_from_inventory);

/// Generate argument names for JS function (a0, a1, a2, ...)
fn generate_args(count: usize) -> String {
    (0..count)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl FunctionRegistry {
    fn collect_from_inventory() -> Self {
        let mut modules = BTreeMap::new();

        // Collect all inline JS modules and deduplicate by content hash
        for inline_js in inventory::iter::<InlineJsModule>() {
            let hash = inline_js.hash();
            let module_path = format!("__wbg__/snippets/{hash}.js");
            // Only insert if we haven't seen this content before
            modules.entry(module_path).or_insert(inline_js.content());
        }

        // Collect all function specs
        let specs: Vec<_> = inventory::iter::<JsFunctionSpec>().copied().collect();

        // Build the script - load modules from wry:// handler before setting up function registry
        let mut script = String::new();

        // Wrap everything in an async IIFE to use await
        script.push_str("(async () => {\n");

        // Track which modules we've already imported (by hash)
        let mut imported_modules = alloc::collections::BTreeSet::new();

        // Load all inline_js modules from the wry handler (deduplicated by content hash)
        for inline_js in inventory::iter::<InlineJsModule>() {
            let hash = inline_js.hash();
            // Only import each unique module once
            if imported_modules.insert(hash.clone()) {
                // Dynamically import the module from /__wbg__/snippets/{hash}.js
                writeln!(
                    &mut script,
                    "  const module_{hash} = await import('/__wbg__/snippets/{hash}.js');"
                )
                .unwrap();
            }
        }

        // Now set up the function registry after all modules are loaded
        // Store raw JS functions - type info will be passed at call time
        script.push_str("  window.setFunctionRegistry([");
        for (i, spec) in specs.iter().enumerate() {
            if i > 0 {
                script.push_str(",\n");
            }
            let js_code = (spec.js_code())();
            write!(&mut script, "{js_code}").unwrap();
        }
        script.push_str("]);\n");

        // Collect all class members and group by class name
        let mut class_members: BTreeMap<&str, Vec<&JsClassMemberSpec>> = BTreeMap::new();
        for member in inventory::iter::<JsClassMemberSpec>() {
            class_members
                .entry(member.class_name())
                .or_default()
                .push(member);
        }

        // Generate complete class definitions for each exported struct
        for (class_name, members) in &class_members {
            // Generate class shell
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
      if (handle !== 0) window.__wryCallExport("{class_name}::__drop", handle);
    }}"#
            )
            .unwrap();

            // Track getters/setters to combine them into single property descriptors
            let mut getters: BTreeMap<&str, &JsClassMemberSpec> = BTreeMap::new();
            let mut setters: BTreeMap<&str, &JsClassMemberSpec> = BTreeMap::new();

            // Generate methods inside the class body
            for member in members {
                match member.kind() {
                    JsClassMemberKind::Method => {
                        // Instance method
                        let args = generate_args(member.arg_count());
                        let args_with_handle = if member.arg_count() > 0 {
                            format!("this.__handle, {args}")
                        } else {
                            "this.__handle".to_string()
                        };
                        writeln!(
                            &mut script,
                            r#"    {}({}) {{ return window.__wryCallExport("{}", {}); }}"#,
                            member.member_name(),
                            args,
                            member.export_name(),
                            args_with_handle
                        )
                        .unwrap();
                    }
                    JsClassMemberKind::Getter => {
                        getters.insert(member.member_name(), member);
                    }
                    JsClassMemberKind::Setter => {
                        setters.insert(member.member_name(), member);
                    }
                    _ => {} // Constructor and static handled separately
                }
            }

            // Generate getters/setters as property accessors inside the class
            let mut property_names: alloc::collections::BTreeSet<&str> =
                alloc::collections::BTreeSet::new();
            property_names.extend(getters.keys());
            property_names.extend(setters.keys());

            for prop_name in property_names {
                let getter = getters.get(prop_name);
                let setter = setters.get(prop_name);
                match (getter, setter) {
                    (Some(g), Some(s)) => {
                        writeln!(
                            &mut script,
                            r#"    get {}() {{ return window.__wryCallExport("{}", this.__handle); }}
    set {}(v) {{ window.__wryCallExport("{}", this.__handle, v); }}"#,
                            prop_name, g.export_name(), prop_name, s.export_name()
                        )
                        .unwrap();
                    }
                    (Some(g), None) => {
                        writeln!(
                            &mut script,
                            r#"    get {}() {{ return window.__wryCallExport("{}", this.__handle); }}"#,
                            prop_name, g.export_name()
                        )
                        .unwrap();
                    }
                    (None, Some(s)) => {
                        writeln!(
                            &mut script,
                            r#"    set {}(v) {{ window.__wryCallExport("{}", this.__handle, v); }}"#,
                            prop_name, s.export_name()
                        )
                        .unwrap();
                    }
                    (None, None) => {}
                }
            }

            // Close the class body
            script.push_str("  }\n");

            // Add static methods and constructors outside the class
            for member in members {
                match member.kind() {
                    JsClassMemberKind::Constructor => {
                        let args = generate_args(member.arg_count());
                        let args_call = if member.arg_count() > 0 { &args } else { "" };
                        writeln!(
                            &mut script,
                            r#"  {class_name}.{method_name} = function({args}) {{ const handle = window.__wryCallExport("{export_name}", {args_call}); return {class_name}.__wrap(handle); }};"#,
                            class_name = class_name,
                            method_name = member.member_name(),
                            args = args,
                            export_name = member.export_name(),
                            args_call = args_call
                        )
                        .unwrap();
                    }
                    JsClassMemberKind::StaticMethod => {
                        let args = generate_args(member.arg_count());
                        let args_call = if member.arg_count() > 0 { &args } else { "" };
                        writeln!(
                            &mut script,
                            r#"  {class_name}.{method_name} = function({args}) {{ return window.__wryCallExport("{export_name}", {args_call}); }};"#,
                            class_name = class_name,
                            method_name = member.member_name(),
                            args = args,
                            export_name = member.export_name(),
                            args_call = args_call
                        )
                        .unwrap();
                    }
                    _ => {} // Methods, getters, setters already handled
                }
            }

            // Register class on window
            writeln!(&mut script, "  window.{class_name} = {class_name};").unwrap();
        }

        // Send a request to wry to notify that the function registry is initialized
        script.push_str("  fetch('/__wbg__/initialized', { method: 'POST', body: [] });\n");

        // Close the async IIFE
        script.push_str("})();\n");

        Self {
            functions: script,
            function_specs: specs,
            modules,
        }
    }

    /// Get a function by name from the registry
    pub fn get_function<F>(&self, spec: JsFunctionSpec) -> Option<JSFunction<F>> {
        let index = self
            .function_specs
            .iter()
            .position(|s| s.js_code() as usize == spec.js_code() as usize)?;
        Some(JSFunction::new(index as _))
    }

    /// Get the initialization script
    pub fn script(&self) -> &str {
        &self.functions
    }

    /// Get the content of an inline_js module by path
    pub fn get_module(&self, path: &str) -> Option<&'static str> {
        self.modules.get(path).copied()
    }
}
