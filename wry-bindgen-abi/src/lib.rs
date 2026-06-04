#![no_std]

extern crate alloc;
extern crate std;

mod callback;
mod encode;
mod ipc;
mod js_ref;
mod object_store;
mod registry;
mod runtime;

pub mod unstable {
    pub use crate::registry::{
        JsClassMemberParts, JsClassMemberSpecRuntime, JsExportSpecRuntime, JsFunctionSpecRuntime,
        JsModuleSpecRuntime,
    };
    pub use crate::runtime::{RuntimeHooks, install_runtime_hooks};

    use crate::{DecodeError, DecodedData, JsRef, ObjectHandle, TypeDef};

    pub trait DecodedDataBytes<'a>: Sized {
        fn from_bytes_unstable(bytes: &'a [u8]) -> Result<Self, DecodeError>;
    }

    impl<'a> DecodedDataBytes<'a> for DecodedData<'a> {
        fn from_bytes_unstable(bytes: &'a [u8]) -> Result<Self, DecodeError> {
            DecodedData::from_bytes(bytes)
        }
    }

    pub trait TypeDefBytes {
        fn bytes(&self) -> &[u8];
    }

    impl TypeDefBytes for TypeDef {
        fn bytes(&self) -> &[u8] {
            TypeDef::bytes(self)
        }
    }

    pub trait JsRefRaw {
        fn from_raw(raw: u64) -> Self;
        fn raw(self) -> u64;
    }

    impl JsRefRaw for JsRef {
        fn from_raw(raw: u64) -> Self {
            JsRef::from_raw_inner(raw)
        }

        fn raw(self) -> u64 {
            self.raw_inner()
        }
    }

    pub trait ObjectHandleRaw {
        fn from_raw(raw: u32) -> Self;
        fn raw(self) -> u32;
    }

    impl ObjectHandleRaw for ObjectHandle {
        fn from_raw(raw: u32) -> Self {
            ObjectHandle::from_raw_inner(raw)
        }

        fn raw(self) -> u32 {
            self.raw_inner()
        }
    }
}

pub use callback::RustCallback;
pub use encode::{
    BinaryDecode, BinaryEncode, EncodeTypeDef, FunctionTypeInfo, JsRefEncode, TypeDef,
};
pub use ipc::{DecodeError, DecodedData, EncodedData};
pub use js_ref::JsRef;
pub use object_store::ObjectHandle;
pub use registry::{
    JsClassMemberKind, JsClassMemberSpec, JsExportSpec, JsFunctionSpec, JsModuleSpec,
};
