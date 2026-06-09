//! Semantic encoding extensions for types owned by `wry-bindgen`.

pub use wry_bindgen_core::{
    Anchored, ArgAbi, BatchableResult, BinaryDecode, BinaryEncode, BorrowScope, CallScoped,
    EncodeTypeDef, JsRef, JsRefEncode, ThrowingResult, TypeDef,
};

// Internal wire marker — used within this module's encoders, not part of the
// public API.
use wry_bindgen_core::RequireFlush;

/// Trait for converting a closure into a Closure wrapper.
/// This trait is used instead of `From` to allow blanket implementations
/// for all closure types without conflicting with other `From` impls.
/// Output is a generic parameter (not associated type) to allow implementing
/// the trait multiple times for the same type with different outputs.
pub(crate) trait IntoClosure<M, Output> {
    fn into_closure(self) -> Output;
}

mod callbacks;
mod values;
