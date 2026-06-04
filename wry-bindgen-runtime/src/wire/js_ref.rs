use super::encode::{BinaryDecode, BinaryEncode, EncodeTypeDef, JsRefEncode, TypeDef};
use super::ipc::{DecodeError, DecodedData, EncodedData};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct JsRef(u64);

impl JsRef {
    /// The `null` JS value constant.
    pub const NULL: Self = Self(129);

    /// The `undefined` JS value constant.
    pub const UNDEFINED: Self = Self(128);

    /// The `true` JS value constant.
    pub const TRUE: Self = Self(130);

    /// The `false` JS value constant.
    pub const FALSE: Self = Self(131);

    #[inline]
    pub const fn is_special_value(self) -> bool {
        self.0 >= Self::UNDEFINED.0 && self.0 <= Self::FALSE.0
    }

    #[inline]
    pub const fn is_owned_heap_ref(self) -> bool {
        self.0 > Self::FALSE.0
    }

    #[inline]
    pub const fn into_abi(self) -> u32 {
        self.0 as u32
    }

    #[inline]
    pub const fn from_abi(abi: u32) -> Self {
        Self(abi as u64)
    }

    /// Reserve a handle to the next borrowed JS reference.
    ///
    /// Borrowed references occupy the borrow stack (indices 1-127) rather than a
    /// heap slot; JS puts the value on its borrow stack without sending an id, so
    /// Rust syncs by reserving the matching handle here.
    #[inline]
    pub fn next_borrowed_ref() -> Self {
        crate::batch::with_runtime(|rt| rt.next_borrowed_ref())
    }

    /// Release this JS heap object.
    #[inline]
    pub fn drop_js_object(self) {
        crate::batch::drop_js_object(self);
    }

    /// Mark the JS wrapper for the Rust callback behind this reference as
    /// unusable.
    #[inline]
    pub fn invalidate_js_rust_function(self) {
        crate::batch::invalidate_js_rust_function(self);
    }

    #[inline]
    pub(crate) const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub(crate) const fn raw(self) -> u64 {
        self.0
    }
}

impl EncodeTypeDef for JsRef {
    fn encode_type_def(type_def: &mut TypeDef) {
        type_def.heap_ref();
    }
}

impl BinaryEncode for JsRef {
    fn encode(self, encoder: &mut EncodedData) {
        self.0.encode(encoder);
    }
}

impl BinaryDecode for JsRef {
    fn decode(_decoder: &mut DecodedData) -> Result<Self, DecodeError> {
        // JS heap references are sent out-of-band in the deferred heap-ref
        // batch. Decoding reserves the next Rust-side ID for that value.
        Ok(crate::batch::with_runtime(|rt| rt.get_next_inbound_js_ref()))
    }
}

impl JsRefEncode for JsRef {
    fn js_ref(&self) -> JsRef {
        *self
    }
}
