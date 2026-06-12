//! Wry-specific `ArgAbi` implementations.
//!
//! The general argument ABI lives in `wry-bindgen-runtime`, where callback
//! encoding can also use it. This module keeps the public
//! `wry_bindgen::convert::{ArgAbi, CallScoped, Anchored}` paths intact and adds
//! the JS-handle borrow cases that require `wry-bindgen`'s `JsValue` anchors.

use crate::JsValue;
use crate::encode::BinaryDecode;
use crate::ipc::{DecodeError, DecodedData};

pub use crate::encode::{Anchored, ArgAbi, BorrowScope, CallScoped};

use super::{JsCastAnchor, OwnedArgAnchor, RefArg, RefMutArg};

// ---------------------------------------------------------------------------
// JS-handle borrows. A shared `&JsValue` is the one shape that differs by scope:
// `CallScoped` rides JS's borrow stack, while `Anchored` decodes an owned handle
// that survives the returned `Promise` (the caller moves the anchor into
// the export's future, so the borrow lives across `.await`s). A `&mut JsValue`
// always decodes an owned handle, so it shares one impl. Imported `extern` types
// get their `ArgAbi` impls from codegen (a blanket over `JsCast` would break the
// orphan rule).
// ---------------------------------------------------------------------------

impl ArgAbi<CallScoped> for &JsValue {
    type Wire = RefArg<JsValue>;
    type Value = ();
    type Anchor = JsCastAnchor<JsValue>;
    type Projected<'a> = &'a JsValue;

    fn decode(_decoder: &mut DecodedData) -> Result<(Self::Value, Self::Anchor), DecodeError> {
        Ok(((), JsCastAnchor::next_borrowed()))
    }

    fn project(_value: Self::Value, anchor: &mut Self::Anchor) -> Self::Projected<'_> {
        anchor
    }
}

impl ArgAbi<Anchored> for &JsValue {
    type Wire = JsValue;
    type Value = ();
    type Anchor = OwnedArgAnchor<JsValue>;
    type Projected<'a> = &'a JsValue;

    fn decode(decoder: &mut DecodedData) -> Result<(Self::Value, Self::Anchor), DecodeError> {
        Ok((
            (),
            OwnedArgAnchor::from_value(<JsValue as BinaryDecode>::decode(decoder)?),
        ))
    }

    fn project(_value: Self::Value, anchor: &mut Self::Anchor) -> Self::Projected<'_> {
        anchor
    }
}

impl<S: BorrowScope> ArgAbi<S> for &mut JsValue {
    type Wire = RefMutArg<JsValue>;
    type Value = ();
    type Anchor = OwnedArgAnchor<JsValue>;
    type Projected<'a> = &'a mut JsValue;

    fn decode(decoder: &mut DecodedData) -> Result<(Self::Value, Self::Anchor), DecodeError> {
        Ok((
            (),
            OwnedArgAnchor::from_value(<JsValue as BinaryDecode>::decode(decoder)?),
        ))
    }

    fn project(_value: Self::Value, anchor: &mut Self::Anchor) -> Self::Projected<'_> {
        anchor
    }
}
