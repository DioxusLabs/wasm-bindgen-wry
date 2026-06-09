use std::boxed::Box;

use alloc::string::String;
use alloc::vec::Vec;

use super::TypeDef;

#[derive(Clone, Copy)]
pub struct JsFunctionSpec {
    code: JsFunctionCode,
}

impl JsFunctionSpec {
    pub const fn new(js_code: fn() -> String) -> Self {
        Self {
            code: JsFunctionCode::Global(js_code),
        }
    }

    pub const fn with_module(module: &'static JsModuleSpec, js_code: fn(&str) -> String) -> Self {
        Self {
            code: JsFunctionCode::Module { module, js_code },
        }
    }
}

#[derive(Clone, Copy)]
enum JsFunctionCode {
    Global(fn() -> String),
    Module {
        module: &'static JsModuleSpec,
        js_code: fn(&str) -> String,
    },
}

inventory::collect!(JsFunctionSpec);

#[derive(Clone, Copy)]
pub struct JsReexportSpec {
    name: &'static str,
    namespace: &'static [&'static str],
    code: JsFunctionCode,
}

impl JsReexportSpec {
    pub const fn new(
        name: &'static str,
        namespace: &'static [&'static str],
        js_code: fn() -> String,
    ) -> Self {
        Self {
            name,
            namespace,
            code: JsFunctionCode::Global(js_code),
        }
    }

    pub const fn with_module(
        module: &'static JsModuleSpec,
        name: &'static str,
        namespace: &'static [&'static str],
        js_code: fn(&str) -> String,
    ) -> Self {
        Self {
            name,
            namespace,
            code: JsFunctionCode::Module { module, js_code },
        }
    }
}

impl JsReexportSpec {
    pub(crate) fn module(&self) -> Option<&'static JsModuleSpec> {
        match self.code {
            JsFunctionCode::Global(_) => None,
            JsFunctionCode::Module { module, .. } => Some(module),
        }
    }

    pub(crate) fn parts(&self) -> (&'static str, &'static [&'static str], String) {
        let value = match self.code {
            JsFunctionCode::Global(js_code) => js_code(),
            JsFunctionCode::Module { module, js_code } => {
                let module_binding = alloc::format!("module_{:x}", module.const_hash());
                js_code(&module_binding)
            }
        };
        (self.name, self.namespace, value)
    }
}

inventory::collect!(JsReexportSpec);

#[derive(Clone, Copy)]
pub struct JsModuleSpec {
    value: &'static str,
    raw: bool,
}

impl JsModuleSpec {
    pub const fn new(content: &'static str) -> Self {
        Self {
            value: content,
            raw: false,
        }
    }

    pub const fn raw(specifier: &'static str) -> Self {
        Self {
            value: specifier,
            raw: true,
        }
    }

    pub const fn const_hash(&self) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET_BASIS;
        let tag = self.raw as u8;
        hash ^= tag as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        let mut i = 0;
        let bytes = self.value.as_bytes();
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        hash
    }
}

#[derive(Clone, Copy)]
pub struct JsClassSpec {
    class_name: &'static str,
    js_name: &'static str,
    js_namespace: &'static [&'static str],
    private: bool,
    extends: Option<&'static str>,
    extends_js_class: Option<&'static str>,
    extends_js_namespace: &'static [&'static str],
    /// `#[wasm_bindgen(inspectable)]`: emit JS `toJSON`/`toString` methods (over
    /// the public field getters) unless the user defines their own.
    inspectable: bool,
    /// The JS names of the public (getter-exposed) fields, for the generated
    /// `toJSON` body when `inspectable`.
    public_fields: &'static [&'static str],
}

impl JsClassSpec {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        class_name: &'static str,
        js_name: &'static str,
        js_namespace: &'static [&'static str],
        private: bool,
        extends: Option<&'static str>,
        extends_js_class: Option<&'static str>,
        extends_js_namespace: &'static [&'static str],
        inspectable: bool,
        public_fields: &'static [&'static str],
    ) -> Self {
        Self {
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
}

pub(super) type JsClassParts = (
    &'static str,
    &'static str,
    &'static [&'static str],
    bool,
    Option<&'static str>,
    Option<&'static str>,
    &'static [&'static str],
    bool,
    &'static [&'static str],
);

impl JsClassSpec {
    pub(crate) fn parts(&self) -> JsClassParts {
        (
            self.class_name,
            self.js_name,
            self.js_namespace,
            self.private,
            self.extends,
            self.extends_js_class,
            self.extends_js_namespace,
            self.inspectable,
            self.public_fields,
        )
    }
}

inventory::collect!(JsClassSpec);

impl JsFunctionSpec {
    pub(crate) fn module(&self) -> Option<&'static JsModuleSpec> {
        match self.code {
            JsFunctionCode::Global(_) => None,
            JsFunctionCode::Module { module, .. } => Some(module),
        }
    }

    pub(crate) fn render_js_code(&self) -> String {
        match self.code {
            JsFunctionCode::Global(js_code) => js_code(),
            JsFunctionCode::Module { module, js_code } => {
                let module_binding = alloc::format!("module_{:x}", module.const_hash());
                js_code(&module_binding)
            }
        }
    }

    pub(crate) fn identity_eq(&self, other: &JsFunctionSpec) -> bool {
        match (self.code, other.code) {
            (JsFunctionCode::Global(a), JsFunctionCode::Global(b)) => a as usize == b as usize,
            (
                JsFunctionCode::Module {
                    module: module_a,
                    js_code: js_code_a,
                },
                JsFunctionCode::Module {
                    module: module_b,
                    js_code: js_code_b,
                },
            ) => core::ptr::eq(module_a, module_b) && js_code_a as usize == js_code_b as usize,
            _ => false,
        }
    }
}

impl JsModuleSpec {
    pub(crate) fn content(&self) -> Option<&'static str> {
        if self.raw { None } else { Some(self.value) }
    }

    pub(crate) fn raw_specifier(&self) -> Option<&'static str> {
        if self.raw { Some(self.value) } else { None }
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum JsClassMemberKind {
    Constructor,
    Method,
    StaticMethod,
    Getter,
    Setter,
    StaticGetter,
    StaticSetter,
}

#[derive(Clone, Copy)]
pub struct JsClassMemberSpec {
    class_name: &'static str,
    member_name: &'static str,
    export_name: &'static str,
    arg_types: fn() -> Vec<TypeDef>,
    return_type: fn() -> TypeDef,
    kind: JsClassMemberKind,
    /// Whether the member takes `self` by value, consuming the receiver. The
    /// generated wrapper zeroes `this.__handle` after the call so a later use
    /// throws "Attempt to use a moved value" (wasm-bindgen's behavior).
    consumes_self: bool,
}

impl JsClassMemberSpec {
    pub const fn new(
        class_name: &'static str,
        member_name: &'static str,
        export_name: &'static str,
        arg_types: fn() -> Vec<TypeDef>,
        return_type: fn() -> TypeDef,
        kind: JsClassMemberKind,
        consumes_self: bool,
    ) -> Self {
        Self {
            class_name,
            member_name,
            export_name,
            arg_types,
            return_type,
            kind,
            consumes_self,
        }
    }
}

pub(super) type JsClassMemberParts = (
    &'static str,
    &'static str,
    &'static str,
    Vec<TypeDef>,
    TypeDef,
    JsClassMemberKind,
    bool,
);

impl JsClassMemberSpec {
    pub(crate) fn parts(&self) -> JsClassMemberParts {
        (
            self.class_name,
            self.member_name,
            self.export_name,
            (self.arg_types)(),
            (self.return_type)(),
            self.kind,
            self.consumes_self,
        )
    }
}

inventory::collect!(JsClassMemberSpec);

#[derive(Clone)]
pub struct JsFunctionArg {
    pub name: &'static str,
    pub ty: TypeDef,
}

#[derive(Clone)]
pub struct JsFunctionSignature {
    name: &'static str,
    namespace: &'static [&'static str],
    args: Vec<JsFunctionArg>,
    return_type: TypeDef,
    this: bool,
    public: bool,
    start: bool,
    /// Whether the export is `#[wasm_bindgen(variadic)]`: the generated JS
    /// wrapper collects its trailing arguments into the final parameter (a rest
    /// parameter) so a spread call like `f(...arr)` reaches Rust as one array.
    variadic: bool,
}

impl JsFunctionSignature {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: &'static str,
        namespace: &'static [&'static str],
        args: Vec<JsFunctionArg>,
        return_type: TypeDef,
        this: bool,
        public: bool,
        start: bool,
        variadic: bool,
    ) -> Self {
        Self {
            name,
            namespace,
            args,
            return_type,
            this,
            public,
            start,
            variadic,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn namespace(&self) -> &'static [&'static str] {
        self.namespace
    }

    pub(crate) fn args(&self) -> &[JsFunctionArg] {
        &self.args
    }

    pub(crate) fn return_type(&self) -> &TypeDef {
        &self.return_type
    }

    pub(crate) fn this(&self) -> bool {
        self.this
    }

    pub(crate) fn public(&self) -> bool {
        self.public
    }

    pub(crate) fn start(&self) -> bool {
        self.start
    }

    pub(crate) fn variadic(&self) -> bool {
        self.variadic
    }
}

pub struct JsExportSpec {
    signature: JsFunctionSignature,
    handler: Box<dyn Fn(&mut super::DecodedData) -> Result<super::EncodedData, String>>,
}

impl JsExportSpec {
    pub fn new(
        signature: JsFunctionSignature,
        handler: impl Fn(&mut super::DecodedData) -> Result<super::EncodedData, String> + 'static,
    ) -> Self {
        Self {
            signature,
            handler: Box::new(handler),
        }
    }

    pub(crate) fn call(
        &self,
        data: &mut super::DecodedData<'_>,
    ) -> Result<super::EncodedData, String> {
        (self.handler)(data)
    }

    pub fn signature(&self) -> &JsFunctionSignature {
        &self.signature
    }
}

#[derive(Clone, Copy)]
pub struct JsExportSpecRegistration {
    register: fn() -> JsExportSpec,
}

impl JsExportSpecRegistration {
    pub const fn new(register: fn() -> JsExportSpec) -> Self {
        Self { register }
    }

    pub(crate) fn spec(&self) -> JsExportSpec {
        (self.register)()
    }
}

inventory::collect!(JsExportSpecRegistration);
