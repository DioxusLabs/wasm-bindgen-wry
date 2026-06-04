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
pub struct JsModuleSpec {
    content: &'static str,
}

impl JsModuleSpec {
    pub const fn new(content: &'static str) -> Self {
        Self { content }
    }

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
    pub(crate) fn content(&self) -> &'static str {
        self.content
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
}

#[derive(Clone, Copy)]
pub struct JsClassMemberSpec {
    class_name: &'static str,
    member_name: &'static str,
    export_name: &'static str,
    arg_count: usize,
    arg_types: fn() -> Vec<TypeDef>,
    return_type: fn() -> Option<TypeDef>,
    kind: JsClassMemberKind,
}

impl JsClassMemberSpec {
    pub const fn new(
        class_name: &'static str,
        member_name: &'static str,
        export_name: &'static str,
        arg_count: usize,
        arg_types: fn() -> Vec<TypeDef>,
        return_type: fn() -> Option<TypeDef>,
        kind: JsClassMemberKind,
    ) -> Self {
        Self {
            class_name,
            member_name,
            export_name,
            arg_count,
            arg_types,
            return_type,
            kind,
        }
    }
}

pub(super) type JsClassMemberParts = (
    &'static str,
    &'static str,
    &'static str,
    usize,
    Vec<TypeDef>,
    Option<TypeDef>,
    JsClassMemberKind,
);

impl JsClassMemberSpec {
    pub(crate) fn parts(&self) -> JsClassMemberParts {
        (
            self.class_name,
            self.member_name,
            self.export_name,
            self.arg_count,
            (self.arg_types)(),
            (self.return_type)(),
            self.kind,
        )
    }
}

inventory::collect!(JsClassMemberSpec);

#[derive(Clone, Copy)]
pub struct JsFreeFunctionSpec {
    js_name: &'static str,
    export_name: &'static str,
    arg_count: usize,
    arg_types: fn() -> Vec<TypeDef>,
    return_type: fn() -> Option<TypeDef>,
}

impl JsFreeFunctionSpec {
    pub const fn new(
        js_name: &'static str,
        export_name: &'static str,
        arg_count: usize,
        arg_types: fn() -> Vec<TypeDef>,
        return_type: fn() -> Option<TypeDef>,
    ) -> Self {
        Self {
            js_name,
            export_name,
            arg_count,
            arg_types,
            return_type,
        }
    }
}

pub(super) type JsFreeFunctionParts = (
    &'static str,
    &'static str,
    usize,
    Vec<TypeDef>,
    Option<TypeDef>,
);

impl JsFreeFunctionSpec {
    pub(crate) fn parts(&self) -> JsFreeFunctionParts {
        (
            self.js_name,
            self.export_name,
            self.arg_count,
            (self.arg_types)(),
            (self.return_type)(),
        )
    }
}

inventory::collect!(JsFreeFunctionSpec);

#[derive(Clone, Copy)]
pub struct JsExportSpec {
    name: &'static str,
    handler: fn(&mut super::DecodedData) -> Result<super::EncodedData, String>,
}

impl JsExportSpec {
    pub const fn new(
        name: &'static str,
        handler: fn(&mut super::DecodedData) -> Result<super::EncodedData, String>,
    ) -> Self {
        Self { name, handler }
    }

    pub(crate) fn call_if_name(
        &self,
        name: &str,
        data: &mut super::DecodedData<'_>,
    ) -> Option<Result<super::EncodedData, String>> {
        (self.name == name).then(|| (self.handler)(data))
    }
}

inventory::collect!(JsExportSpec);
