//! Type-directed decode of exported-function arguments.
//!
//! The proc-macro emits one uniform projection per argument —
//! `<#arg_ty as ArgAbi<S>>::decode` / `project` / `write_back` — keyed on the
//! *full spelled* argument type. Trait resolution, which sees through type
//! aliases, then selects the behavior, so `fn f(x: U8Slice)` with `type
//! U8Slice<'a> = &'a [u8]` decodes identically to `fn f(x: &[u8])`. This mirrors
//! how [`ReturnWasmAbi`](super::ReturnWasmAbi) already makes the return position
//! alias-transparent.
//!
//! The [borrow scope](BorrowScope) `S` distinguishes a synchronous call
//! ([`CallScoped`]) from an `async` export ([`Anchored`]): the two differ only
//! for JS-handle borrows, where a sync borrow may ride JS's borrow stack but an
//! async one must anchor an owned copy that outlives the returned `Promise`.
//! Every other argument shape behaves the same in both scopes and so has a
//! single `impl<S: BorrowScope>`.
//!
//! Each impl delegates to the existing borrow/owned traits
//! ([`RefFromBinaryDecode`], [`MutSliceArg`], [`BinaryDecode`]) so the advertised
//! wire type and decoded bytes are unchanged from the previous syntactic codegen.
//!
//! The projected lifetime is a [generic associated type](ArgAbi::Projected)
//! rather than a trait parameter, so the macro can name `<&[u8] as
//! ArgAbi<S>>::Wire` without threading a lifetime through every type position.

use crate::__rt::{
    BinaryDecode, DecodeError, DecodedData, EncodeTypeDef, EncodedData, MutSliceArg,
};
use crate::JsValue;

use super::{FromWasmAbi, JsCastAnchor, OwnedArgAnchor, RefArg, RefFromBinaryDecode, RefMutArg};

mod sealed {
    pub trait Sealed {}
}

/// How long a decoded borrow must remain valid — the type-level switch between
/// the synchronous and `async` decode of an argument.
pub trait BorrowScope: sealed::Sealed {}

/// The borrow only needs to outlive the synchronous call, so a JS handle may ride
/// JS's borrow stack.
pub struct CallScoped;

/// The borrow must outlive the `Promise` an `async` export returns, so a JS handle
/// anchors an owned copy instead of riding the borrow stack.
pub struct Anchored;

impl sealed::Sealed for CallScoped {}
impl sealed::Sealed for Anchored {}
impl BorrowScope for CallScoped {}
impl BorrowScope for Anchored {}

/// Decode one exported-function argument from the wire, for borrow scope `S`.
///
/// [`decode`](ArgAbi::decode) produces a [`Guard`](ArgAbi::Guard) that owns
/// whatever backs the argument for the duration of the call;
/// [`project`](ArgAbi::project) lends the [`Projected`](ArgAbi::Projected) value
/// the exported function actually receives (`&T`, `&mut T`, or an owned `T`) out
/// of that guard.
pub trait ArgAbi<S: BorrowScope> {
    /// The type whose `TypeDef` is advertised to JS for this argument.
    type Wire: EncodeTypeDef;

    /// Owns whatever keeps the projected argument valid across the call.
    type Guard;

    /// What the exported function receives, borrowed from the guard for `'a`.
    type Projected<'a>
    where
        Self: 'a;

    /// Decode the guard from the incoming wire bytes.
    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError>;

    /// Lend the call argument out of the guard.
    fn project(guard: &mut Self::Guard) -> Self::Projected<'_>;

    /// Append any post-call write-back data to the export response.
    fn write_back(_guard: Self::Guard, _encoder: &mut EncodedData) {}
}

// ---------------------------------------------------------------------------
// Owned values: anything that decodes by value (de-blanketed `FromWasmAbi`,
// which never covers a reference type, so this does not overlap the borrow
// impls below). Identical in both scopes.
// ---------------------------------------------------------------------------

impl<S: BorrowScope, T: FromWasmAbi> ArgAbi<S> for T {
    type Wire = T;
    type Guard = Option<T>;
    type Projected<'a>
        = T
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(Some(<T as BinaryDecode>::decode(decoder)?))
    }

    fn project(guard: &mut Self::Guard) -> T {
        guard
            .take()
            .expect("an export argument is projected exactly once")
    }
}

// ---------------------------------------------------------------------------
// Shared borrows of owned-transport types: `&str` and `&[T]` decode an owned
// copy and lend a reference into it. The owned copy already outlives any await,
// so both scopes share one impl.
// ---------------------------------------------------------------------------

impl<S: BorrowScope> ArgAbi<S> for &str {
    type Wire = <str as RefFromBinaryDecode>::Wire;
    type Guard = <str as RefFromBinaryDecode>::Anchor;
    type Projected<'a>
        = &'a str
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        <str as RefFromBinaryDecode>::ref_decode(decoder)
    }

    fn project(guard: &mut Self::Guard) -> &str {
        guard
    }
}

impl<S: BorrowScope, T> ArgAbi<S> for &[T]
where
    T: BinaryDecode + EncodeTypeDef,
{
    type Wire = <[T] as RefFromBinaryDecode>::Wire;
    type Guard = <[T] as RefFromBinaryDecode>::Anchor;
    type Projected<'a>
        = &'a [T]
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        <[T] as RefFromBinaryDecode>::ref_decode(decoder)
    }

    fn project(guard: &mut Self::Guard) -> &[T] {
        guard
    }
}

// ---------------------------------------------------------------------------
// Mutable slice borrow: decodes into a `MutSliceArg` guard whose contents are
// written back to JS after the return value. Identical in both scopes.
// ---------------------------------------------------------------------------

impl<S: BorrowScope, T> ArgAbi<S> for &mut [T]
where
    T: BinaryDecode + crate::__rt::BinaryEncode + EncodeTypeDef,
{
    type Wire = MutSliceArg<T>;
    type Guard = MutSliceArg<T>;
    type Projected<'a>
        = &'a mut [T]
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        MutSliceArg::decode(decoder)
    }

    fn project(guard: &mut Self::Guard) -> &mut [T] {
        guard
    }

    fn write_back(guard: Self::Guard, encoder: &mut EncodedData) {
        MutSliceArg::write_back(guard, encoder);
    }
}

// ---------------------------------------------------------------------------
// JS-handle borrows. A shared `&JsValue` is the one shape that differs by scope:
// `CallScoped` rides JS's borrow stack, while `Anchored` decodes an owned handle
// that survives the returned `Promise`. A `&mut JsValue` always decodes an owned
// handle, so it shares one impl. Imported `extern` types get their `ArgAbi` impls
// from codegen (a blanket over `JsCast` would break the orphan rule).
// ---------------------------------------------------------------------------

impl ArgAbi<CallScoped> for &JsValue {
    type Wire = RefArg<JsValue>;
    type Guard = JsCastAnchor<JsValue>;
    type Projected<'a>
        = &'a JsValue
    where
        Self: 'a;

    fn decode(_decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(JsCastAnchor::next_borrowed())
    }

    fn project(guard: &mut Self::Guard) -> &JsValue {
        guard
    }
}

impl ArgAbi<Anchored> for &JsValue {
    type Wire = JsValue;
    type Guard = OwnedArgAnchor<JsValue>;
    type Projected<'a>
        = &'a JsValue
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(OwnedArgAnchor::from_value(
            <JsValue as BinaryDecode>::decode(decoder)?,
        ))
    }

    fn project(guard: &mut Self::Guard) -> &JsValue {
        guard
    }
}

impl<S: BorrowScope> ArgAbi<S> for &mut JsValue {
    type Wire = RefMutArg<JsValue>;
    type Guard = OwnedArgAnchor<JsValue>;
    type Projected<'a>
        = &'a mut JsValue
    where
        Self: 'a;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(OwnedArgAnchor::from_value(
            <JsValue as BinaryDecode>::decode(decoder)?,
        ))
    }

    fn project(guard: &mut Self::Guard) -> &mut JsValue {
        guard
    }
}
