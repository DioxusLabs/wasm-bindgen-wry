//! Wry-specific `ArgAbi` implementations.
//!
//! The general argument ABI lives in `wry-bindgen-runtime`, where callback
//! encoding can also use it. This module keeps the public
//! `wry_bindgen::convert::{ArgAbi, CallScoped, Anchored}` paths intact and adds
//! the JS-handle borrow cases that require `wry-bindgen`'s `JsValue` anchors.

use core::ops::{AsyncFnOnce, Deref, DerefMut};

use crate::JsValue;
use crate::encode::BinaryDecode;
use crate::ipc::{DecodeError, DecodedData};

pub use crate::encode::{Anchored, ArgAbi, BorrowScope, CallScoped};

use super::{JsCastAnchor, OwnedArgAnchor, RefArg, RefMutArg};

#[doc(hidden)]
pub fn __wry_project_ref_async<G, R, F>(guard: G, with: F) -> impl core::future::Future<Output = R>
where
    G: Deref,
    F: for<'a> AsyncFnOnce(&'a G::Target) -> R,
{
    async move { with(&*guard).await }
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
    type ProjectedGuard = ();
    type Projected<'a> = &'a JsValue;

    fn decode(_decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(JsCastAnchor::next_borrowed())
    }

    fn project<R, F>(guard: Self::Guard, with: F) -> (R, Self::ProjectedGuard)
    where
        F: for<'a> FnOnce(Self::Projected<'a>) -> R,
    {
        let result = with(&guard);
        (result, ())
    }

    // `CallScoped` is the synchronous scope, so this is never reached by an
    // `async` export; it satisfies the trait by lending the borrow-stack guard.
    async fn project_async<R, F>(guard: Self::Guard, with: F) -> R
    where
        F: for<'a> AsyncFnOnce(Self::Projected<'a>) -> R,
    {
        with(&guard).await
    }
}

impl ArgAbi<Anchored> for &JsValue {
    type Wire = JsValue;
    type Guard = OwnedArgAnchor<JsValue>;
    type ProjectedGuard = Self::Guard;
    type Projected<'a> = &'a JsValue;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(OwnedArgAnchor::from_value(
            <JsValue as BinaryDecode>::decode(decoder)?,
        ))
    }

    fn project<R, F>(guard: Self::Guard, with: F) -> (R, Self::ProjectedGuard)
    where
        F: for<'a> FnOnce(Self::Projected<'a>) -> R,
    {
        let result = with(&guard);
        (result, guard)
    }

    // The owned-handle anchor survives the `Promise`: the future holds it and
    // lends a `&JsValue` across the `.await`.
    async fn project_async<R, F>(guard: Self::Guard, with: F) -> R
    where
        F: for<'a> AsyncFnOnce(Self::Projected<'a>) -> R,
    {
        with(&guard).await
    }
}

impl<S: BorrowScope> ArgAbi<S> for &mut JsValue {
    type Wire = RefMutArg<JsValue>;
    type Guard = OwnedArgAnchor<JsValue>;
    type ProjectedGuard = Self::Guard;
    type Projected<'a> = &'a mut JsValue;

    fn decode(decoder: &mut DecodedData) -> Result<Self::Guard, DecodeError> {
        Ok(OwnedArgAnchor::from_value(
            <JsValue as BinaryDecode>::decode(decoder)?,
        ))
    }

    fn project<R, F>(mut guard: Self::Guard, with: F) -> (R, Self::ProjectedGuard)
    where
        F: for<'a> FnOnce(Self::Projected<'a>) -> R,
    {
        let result = with(guard.deref_mut());
        (result, guard)
    }

    // The owned-handle anchor survives the `Promise`: the future holds it and
    // lends `&mut JsValue` across the `.await`.
    async fn project_async<R, F>(mut guard: Self::Guard, with: F) -> R
    where
        F: for<'a> AsyncFnOnce(Self::Projected<'a>) -> R,
    {
        with(guard.deref_mut()).await
    }
}
