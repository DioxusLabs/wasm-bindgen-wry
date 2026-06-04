//! The hook seam the runtime installs.
//!
//! Encoding lives in this crate, but a few operations — storing a Rust callback,
//! queuing its drop, and reserving an inbound [`JsRef`] — must run against the
//! active runtime. The runtime lives in a crate that depends on this one, so it
//! installs a small table of function pointers ([`RuntimeHooks`]) that the
//! encoding paths call through.

use alloc::boxed::Box;
use core::any::Any;
use core::cell::Cell;
use std::thread_local;

use crate::{JsRef, ObjectHandle};

/// Runtime operations the encoding layer reaches through.
///
/// The runtime installs this table once it is active; the closure-encoding and
/// [`JsRef`] paths call through it. Each hook runs against whatever runtime is
/// currently active on this thread.
pub struct RuntimeHooks {
    /// Store a Rust value in the runtime's object store, returning its handle.
    pub insert_object: fn(Box<dyn Any>) -> ObjectHandle,
    /// Queue a stored object for drop once the current operation completes.
    pub queue_rust_object_drop: fn(ObjectHandle),
    /// Reserve the next Rust-side id for a JS value arriving out-of-band.
    pub next_inbound_js_ref: fn() -> JsRef,
}

thread_local! {
    static HOOKS: Cell<Option<&'static RuntimeHooks>> = const { Cell::new(None) };
}

/// Install the runtime hook table for the current thread. The runtime calls
/// this whenever it becomes active.
pub fn install_runtime_hooks(hooks: &'static RuntimeHooks) {
    HOOKS.with(|cell| cell.set(Some(hooks)));
}

fn hooks() -> &'static RuntimeHooks {
    HOOKS
        .with(|cell| cell.get())
        .expect("runtime hooks not installed")
}

pub(crate) fn insert_object(obj: Box<dyn Any>) -> ObjectHandle {
    (hooks().insert_object)(obj)
}

pub(crate) fn queue_rust_object_drop(handle: ObjectHandle) {
    (hooks().queue_rust_object_drop)(handle)
}

pub(crate) fn next_inbound_js_ref() -> JsRef {
    (hooks().next_inbound_js_ref)()
}
