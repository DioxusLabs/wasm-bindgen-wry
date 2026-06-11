//! Batching system for grouping multiple JS operations into single messages.
//!
//! This module provides the batching infrastructure that allows multiple
//! JS operations to be grouped together for efficient execution.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::cell::{Cell, Ref, RefCell, RefMut};
use core::ops::{Deref, DerefMut};
use ouroboros::self_referencing;
use std::boxed::Box;
use std::thread_local;

use crate::encode::BinaryDecode;
use crate::id_allocator::{BorrowIds, HeapIds, IdSlab, InstallIdBatch};
use crate::ipc::DecodedData;
use crate::ipc::{EncodedData, EncodedParts, IPCMessage, MessageType};
use crate::object_store::ObjectHandle;
use crate::runtime::WryIPC;
use crate::type_cache::TypeCache;
use crate::wire::BinaryEncode as AbiBinaryEncode;
use crate::wire::{JsFunctionSpec, JsRef, TypeDef};

thread_local! {
    /// Write-backs queued while encoding the current synchronous JS call's
    /// arguments. A `&mut [T]` argument passed to a JS import registers one
    /// here; after the response's return value is decoded, each is run against
    /// the same decoder to copy the (possibly mutated) array back into the
    /// caller's slice. The whole encode -> IPC -> decode -> write-back sequence
    /// runs on one synchronous stack frame inside `run_js_sync`, so the closures
    /// (which capture the caller's slice) never outlive it.
    static PENDING_WRITE_BACKS: RefCell<Vec<Box<dyn FnOnce(&mut DecodedData)>>> =
        const { RefCell::new(Vec::new()) };
}

/// Queue a write-back to run after the current JS call's return value is decoded
/// (see [`PENDING_WRITE_BACKS`]). Called by `&mut [T]` argument encoders.
pub fn push_write_back(write_back: Box<dyn FnOnce(&mut DecodedData)>) {
    PENDING_WRITE_BACKS.with(|cell| cell.borrow_mut().push(write_back));
}

/// Take all queued write-backs, draining the queue.
fn take_write_backs() -> Vec<Box<dyn FnOnce(&mut DecodedData)>> {
    PENDING_WRITE_BACKS.with(|cell| core::mem::take(&mut *cell.borrow_mut()))
}

/// Whether the current call queued any `&mut [T]` write-backs.
fn has_pending_write_backs() -> bool {
    PENDING_WRITE_BACKS.with(|cell| !cell.borrow().is_empty())
}

/// Run any queued write-backs against `data` (the decoded response), in the
/// order their arguments were encoded — matching how the JS side appends the
/// mutated arrays after the return value.
fn run_write_backs(data: &mut DecodedData) {
    for write_back in take_write_backs() {
        write_back(data);
    }
}

/// One operation's deferred cleanup: heap slots and Rust object handles whose
/// release was requested while the operation was encoding. Both are flushed
/// together once the operation completes.
#[derive(Default)]
pub(crate) struct OperationFreeFrame {
    /// Released heap slots to drop in JS and recycle at flush end.
    heap_ids: Vec<u64>,
    /// Rust object handles to remove from the store at flush end.
    object_handles: Vec<u32>,
}

const RECURSIVE_USE: &str = "recursive use of an object";

type ObjectBorrow<'a, T> = Ref<'a, T>;
type ObjectBorrowMut<'a, T> = RefMut<'a, T>;

struct ObjectSlot {
    type_id: TypeId,
    pending_drop: Cell<bool>,
    value: RefCell<Box<dyn Any>>,
}

impl ObjectSlot {
    fn new(obj: Box<dyn Any>) -> Self {
        let type_id = (*obj).type_id();
        Self {
            type_id,
            pending_drop: Cell::new(false),
            value: RefCell::new(obj),
        }
    }

    fn is<T: 'static>(&self) -> bool {
        !self.pending_drop.get() && self.type_id == TypeId::of::<T>()
    }

    fn into_object(self) -> Box<dyn Any> {
        self.value.into_inner()
    }
}

#[self_referencing(no_doc)]
struct ObjectRefCell<T: 'static> {
    slot: Rc<ObjectSlot>,

    #[borrows(slot)]
    #[covariant]
    borrow: ObjectBorrow<'this, T>,
}

#[self_referencing(no_doc)]
struct ObjectRefMutCell<T: 'static> {
    slot: Rc<ObjectSlot>,

    #[borrows(slot)]
    #[not_covariant]
    borrow: ObjectBorrowMut<'this, T>,
}

/// Why an object borrow failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectBorrowError {
    InvalidHandle,
    RecursiveUse,
    TypeMismatch,
}

impl ObjectBorrowError {
    pub fn message(self) -> &'static str {
        match self {
            ObjectBorrowError::InvalidHandle => "invalid handle",
            ObjectBorrowError::RecursiveUse => RECURSIVE_USE,
            ObjectBorrowError::TypeMismatch => "object type mismatch",
        }
    }
}

/// Why taking ownership of an object failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectTakeError {
    InvalidHandle,
    Borrowed,
    TypeMismatch,
}

impl ObjectTakeError {
    pub fn message(self) -> &'static str {
        match self {
            ObjectTakeError::InvalidHandle => "invalid handle",
            ObjectTakeError::Borrowed => {
                "attempted to take ownership of Rust value while it was borrowed"
            }
            ObjectTakeError::TypeMismatch => "object type mismatch",
        }
    }
}

/// Shared borrow of a stored Rust object.
pub struct ObjectRef<T: 'static> {
    handle: ObjectHandle,
    inner: Option<ObjectRefCell<T>>,
}

impl<T: 'static> ObjectRef<T> {
    fn new(handle: ObjectHandle, slot: Rc<ObjectSlot>) -> Result<Self, ObjectBorrowError> {
        Ok(Self {
            handle,
            inner: Some(
                ObjectRefCellTryBuilder {
                    slot,
                    borrow_builder: |slot| {
                        let borrow = slot
                            .value
                            .try_borrow()
                            .map_err(|_| ObjectBorrowError::RecursiveUse)?;
                        Ok(Ref::map(borrow, |value| {
                            value.downcast_ref::<T>().expect("object type mismatch")
                        }))
                    },
                }
                .try_build()?,
            ),
        })
    }
}

impl<T: 'static> Deref for ObjectRef<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner
            .as_ref()
            .expect("object borrow missing")
            .with_borrow(|borrow| &**borrow)
    }
}

impl<T: 'static> Drop for ObjectRef<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let slot = inner.into_heads().slot;
            if slot.pending_drop.get() {
                finish_deferred_object_drop(self.handle, slot);
            }
        }
    }
}

/// Mutable borrow of a stored Rust object.
pub struct ObjectRefMut<T: 'static> {
    handle: ObjectHandle,
    inner: Option<ObjectRefMutCell<T>>,
}

impl<T: 'static> ObjectRefMut<T> {
    fn new(handle: ObjectHandle, slot: Rc<ObjectSlot>) -> Result<Self, ObjectBorrowError> {
        Ok(Self {
            handle,
            inner: Some(
                ObjectRefMutCellTryBuilder {
                    slot,
                    borrow_builder: |slot| {
                        let borrow = slot
                            .value
                            .try_borrow_mut()
                            .map_err(|_| ObjectBorrowError::RecursiveUse)?;
                        Ok(RefMut::map(borrow, |value| {
                            value.downcast_mut::<T>().expect("object type mismatch")
                        }))
                    },
                }
                .try_build()?,
            ),
        })
    }
}

impl<T: 'static> Deref for ObjectRefMut<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner
            .as_ref()
            .expect("object borrow missing")
            .with_borrow(|borrow| &**borrow)
    }
}

impl<T: 'static> DerefMut for ObjectRefMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner
            .as_mut()
            .expect("object borrow missing")
            .with_borrow_mut(|borrow| &mut **borrow)
    }
}

impl<T: 'static> Drop for ObjectRefMut<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let slot = inner.into_heads().slot;
            if slot.pending_drop.get() {
                finish_deferred_object_drop(self.handle, slot);
            }
        }
    }
}
/// State for batching operations and object storage.
/// Every evaluation is a batch - it may just have one operation.
///
/// Also stores exported Rust structs and callback functions.
pub struct Runtime {
    /// The encoder accumulating batched operation payloads.
    encoder: EncodedParts,
    /// Whether the accumulated encoder has at least one operation payload.
    encoder_has_pending_ops: bool,
    /// Heap IDs that mirror the JS runtime's reference slab.
    heap_ids: HeapIds,
    /// Borrow-stack IDs for borrowed references within an operation.
    borrow_ids: BorrowIds,
    /// Handles for Rust-owned exported objects. Rust controls allocation, so
    /// handles are freed (released and recycled) immediately on removal.
    object_handles: IdSlab<u32>,
    /// Whether we're inside a batch() call
    is_batching: bool,
    /// Function-type definitions JS has been told about.
    type_cache: TypeCache,
    /// Exported Rust structs and callbacks stored by handle.
    objects: BTreeMap<u32, Rc<ObjectSlot>>,
    /// Per-operation deferred cleanup frames. Each in-flight operation pushes
    /// one frame; released heap IDs and object handles accumulate into the top
    /// frame and are flushed when the operation completes.
    op_free_stack: Vec<OperationFreeFrame>,
    /// Heap IDs that have been dropped in JS but cannot be recycled until the
    /// current outbound message has been flushed.
    heap_ids_to_recycle_after_flush: Vec<JsRef>,
    /// Function type IDs emitted as full definitions in the current outbound.
    pending_type_ids: Vec<u32>,
    /// The ipc layer used to communicate with the JS runtime
    ipc: WryIPC,
    /// Thread locals associated with the runtime
    thread_locals: BTreeMap<*const (), Box<dyn Any>>,
}

impl Runtime {
    pub(crate) fn new(ipc: WryIPC) -> Self {
        Self {
            encoder: EncodedParts::default(),
            encoder_has_pending_ops: false,
            heap_ids: HeapIds::new(),
            borrow_ids: BorrowIds::new(),
            object_handles: IdSlab::new(1),
            is_batching: false,
            type_cache: TypeCache::new(),
            // Object store starts empty
            objects: BTreeMap::new(),
            op_free_stack: Vec::new(),
            heap_ids_to_recycle_after_flush: Vec::new(),
            pending_type_ids: Vec::new(),
            ipc,
            thread_locals: BTreeMap::new(),
        }
    }

    /// Get a reference to the IPC layer.
    pub(crate) fn ipc(&self) -> &WryIPC {
        &self.ipc
    }

    /// Get the next heap ID for a return value placeholder.
    pub(crate) fn get_next_placeholder_id(&mut self) -> u64 {
        self.heap_ids.next_placeholder_id()
    }

    /// Allocate the next ID for a JS object sent without encoding an ID. The ID
    /// joins the pending install batch shipped on the next Rust-to-JS message.
    pub(crate) fn get_next_inbound_js_heap_id(&mut self) -> u64 {
        self.heap_ids.next_inbound_js_heap_id()
    }

    /// Get the next borrow ID from the borrow stack (indices 1-127).
    /// The borrow stack grows downward from JSIDX_OFFSET (128) toward 1.
    /// Panics if the borrow stack overflows (more than 127 borrowed refs in one operation).
    pub(crate) fn get_next_borrow_id(&mut self) -> u64 {
        self.borrow_ids.next_borrow_id()
    }

    /// Push a borrow frame before a nested operation that may use borrowed refs.
    /// This saves the current borrow stack pointer so we can restore it later.
    pub(crate) fn push_borrow_frame(&mut self) {
        self.borrow_ids.push_frame();
    }

    /// Pop a borrow frame after a nested operation completes.
    /// This restores the borrow stack pointer to where it was before the nested operation.
    pub(crate) fn pop_borrow_frame(&mut self) {
        self.borrow_ids.pop_frame();
    }

    /// Track a heap ID as released and queue it for JS drop when appropriate.
    /// Returns the ID when there is no open operation frame to batch it into,
    /// signalling the caller to notify JS immediately.
    pub(crate) fn release_heap_id(&mut self, id: u64) -> Option<u64> {
        self.heap_ids.release_heap_slot(id);
        match self.op_free_stack.last_mut() {
            Some(frame) => {
                frame.heap_ids.push(id);
                None
            }
            None => Some(id),
        }
    }

    pub(crate) fn recycle_heap_id(&mut self, id: u64) {
        self.heap_ids.recycle_heap_id(id);
    }

    pub(crate) fn recycle_heap_id_if_released(&mut self, id: u64) -> bool {
        self.heap_ids.recycle_heap_id_if_released(id)
    }

    pub(crate) fn defer_heap_id_recycle_until_flush(&mut self, id: u64) {
        self.heap_ids_to_recycle_after_flush
            .push(JsRef::from_raw(id));
    }

    /// Take the message data and reset the batch for reuse.
    /// Includes ID installation and placeholder reservation metadata at the start of the message.
    pub(crate) fn take_message(&mut self) -> (IPCMessage, Vec<JsRef>) {
        let reserved_ids = self
            .take_reserved_placeholder_ids()
            .into_iter()
            .map(JsRef::from_raw)
            .collect::<Vec<_>>();
        let encoder = self.take_encoder();
        let heap_ids_to_recycle_after_flush =
            core::mem::take(&mut self.heap_ids_to_recycle_after_flush);
        (
            self.finish_rust_to_js_message(MessageType::Evaluate, encoder, Some(&reserved_ids)),
            heap_ids_to_recycle_after_flush,
        )
    }

    /// Add Rust-to-JS response metadata and turn the encoder into a response message.
    pub(crate) fn finish_respond_message(&mut self, encoder: EncodedData) -> IPCMessage {
        self.finish_rust_to_js_message(
            MessageType::Respond,
            EncodedParts::from_encoded(encoder),
            None,
        )
    }

    /// Build a response carrying an error string; JS throws it as an exception.
    pub(crate) fn finish_respond_error_message(&mut self, message: &str) -> IPCMessage {
        let mut encoder = EncodedData::default();
        AbiBinaryEncode::encode(message, &mut encoder);
        self.finish_rust_to_js_message(
            MessageType::RespondError,
            EncodedParts::from_encoded(encoder),
            None,
        )
    }

    fn finish_rust_to_js_message(
        &mut self,
        message_type: MessageType,
        encoder: EncodedParts,
        reserved_ids: Option<&[JsRef]>,
    ) -> IPCMessage {
        let install_ids = self
            .take_pending_install_ids()
            .into_iter()
            .map(JsRef::from_raw)
            .collect::<Vec<_>>();
        let mut prelude = Vec::new();
        push_ref_list(&mut prelude, &install_ids);
        if let Some(reserved_ids) = reserved_ids {
            push_ref_list(&mut prelude, reserved_ids);
        }
        let pending_type_ids = core::mem::take(&mut self.pending_type_ids);
        // Reserved-ids is only passed for outbound Evaluates; Responds pass
        // None. Evaluates push a type-cache frame that the matching inbound JS
        // Respond pops and acks. Responds have no such closing message, but JS
        // processes a Respond synchronously before Rust runs again, so any types
        // it introduced are already cached — ack them now rather than dropping
        // them (otherwise they re-ship as TYPE_FULL on every later use).
        if reserved_ids.is_some() {
            self.type_cache.push_pending_frame(pending_type_ids);
        } else {
            self.type_cache.ack_type_ids(&pending_type_ids);
        }
        encoder.into_message(message_type, &prelude)
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.encoder_has_pending_ops
    }

    pub(crate) fn push_operation_frame(&mut self) {
        self.op_free_stack.push(OperationFreeFrame::default());
    }

    pub(crate) fn release_object_handle(&mut self, handle: ObjectHandle) -> Option<Box<dyn Any>> {
        match self.op_free_stack.last_mut() {
            Some(frame) => {
                frame.object_handles.push(handle.raw());
                None
            }
            None => self.drop_object_handle(handle),
        }
    }

    /// Pop the current operation's free frame. If a parent frame exists, the
    /// released IDs and handles collapse into it (so nested ops flush with
    /// their parent) and an empty frame is returned; otherwise the frame is
    /// handed back for the caller to flush.
    pub(crate) fn pop_operation_frame(&mut self) -> OperationFreeFrame {
        let frame = self
            .op_free_stack
            .pop()
            .expect("pop_operation_frame called with empty frame stack");

        if let Some(parent) = self.op_free_stack.last_mut() {
            parent.heap_ids.extend(frame.heap_ids);
            parent.object_handles.extend(frame.object_handles);
            OperationFreeFrame::default()
        } else {
            frame
        }
    }

    pub(crate) fn set_batching(&mut self, batching: bool) {
        self.is_batching = batching;
    }

    pub(crate) fn is_batching(&self) -> bool {
        self.is_batching
    }

    /// Take the IDs JS should install for objects it sent to Rust.
    pub(crate) fn take_pending_install_ids(&mut self) -> InstallIdBatch {
        self.heap_ids.take_pending_install_ids()
    }

    /// Take IDs JS should reserve for pending Rust-to-JS return values.
    pub(crate) fn take_reserved_placeholder_ids(&mut self) -> Vec<u64> {
        self.heap_ids.take_reserved_placeholder_ids()
    }

    pub(crate) fn take_encoder(&mut self) -> EncodedParts {
        let next = EncodedParts::default();
        self.encoder_has_pending_ops = false;
        core::mem::replace(&mut self.encoder, next)
    }

    pub(crate) fn extend_encoder(&mut self, other: EncodedData) {
        self.encoder.append_encoded(other);
        self.encoder_has_pending_ops = true;
    }

    /// Get or create a type ID for a function-type definition. The second
    /// element is true if JS has already acked a `TYPE_FULL` for this ID.
    pub fn get_or_create_type_id(&mut self, type_def: &TypeDef) -> (u32, bool) {
        let (id, can_use_cached) = self.type_cache.get_or_create_type_id(type_def);
        if !can_use_cached {
            self.pending_type_ids.push(id);
        }
        (id, can_use_cached)
    }

    /// Pop the top pending-ack frame and mark its type IDs as acked. Called
    /// when an inbound JS Respond arrives.
    pub(crate) fn pop_and_ack_type_cache_frame(&mut self) {
        self.type_cache.pop_and_ack_pending_frame();
    }

    /// Clone a stored object by handle, returning `None` instead of panicking when
    /// the handle is absent or mutably borrowed. Used by the callback dispatcher
    /// so a call to a dropped or re-entered closure surfaces as a catchable error.
    pub(crate) fn try_clone_object<T: Clone + 'static>(&self, handle: u32) -> Option<T> {
        let slot = self.objects.get(&handle)?;
        if !slot.is::<T>() {
            return None;
        }
        let borrow = slot.value.try_borrow().ok()?;
        borrow.downcast_ref::<T>().cloned()
    }

    /// Whether the object currently stored at `handle` is of type `T`.
    pub fn object_is<T: 'static>(&self, handle: ObjectHandle) -> bool {
        self.objects
            .get(&handle.raw())
            .is_some_and(|slot| slot.is::<T>())
    }

    fn slot_for<T: 'static>(
        &self,
        handle: ObjectHandle,
    ) -> Result<Rc<ObjectSlot>, ObjectBorrowError> {
        let slot = self
            .objects
            .get(&handle.raw())
            .ok_or(ObjectBorrowError::InvalidHandle)?;
        if slot.pending_drop.get() {
            return Err(ObjectBorrowError::InvalidHandle);
        }
        if slot.type_id != TypeId::of::<T>() {
            return Err(ObjectBorrowError::TypeMismatch);
        }
        Ok(Rc::clone(slot))
    }

    pub fn object_ref<T: 'static>(
        &self,
        handle: ObjectHandle,
    ) -> Result<ObjectRef<T>, ObjectBorrowError> {
        ObjectRef::new(handle, self.slot_for::<T>(handle)?)
    }

    pub fn object_mut<T: 'static>(
        &self,
        handle: ObjectHandle,
    ) -> Result<ObjectRefMut<T>, ObjectBorrowError> {
        ObjectRefMut::new(handle, self.slot_for::<T>(handle)?)
    }

    pub fn remove_object_untyped(
        &mut self,
        handle: ObjectHandle,
    ) -> Result<Box<dyn Any>, ObjectTakeError> {
        let raw = handle.raw();
        let slot = Rc::clone(
            self.objects
                .get(&raw)
                .ok_or(ObjectTakeError::InvalidHandle)?,
        );
        if slot.pending_drop.get() {
            return Err(ObjectTakeError::InvalidHandle);
        }
        let value = slot
            .value
            .try_borrow_mut()
            .map_err(|_| ObjectTakeError::Borrowed)?;
        drop(value);
        let object = self
            .remove_ready_object_slot(raw, slot)
            .expect("object slot disappeared after lookup");
        Ok(object)
    }

    pub(crate) fn drop_object_handle(&mut self, handle: ObjectHandle) -> Option<Box<dyn Any>> {
        let raw = handle.raw();
        let slot = Rc::clone(self.objects.get(&raw)?);
        if slot.pending_drop.replace(true) {
            return None;
        }
        let Ok(value) = slot.value.try_borrow_mut() else {
            return None;
        };
        drop(value);
        self.remove_ready_object_slot(raw, slot)
    }

    fn remove_ready_object_slot(&mut self, raw: u32, slot: Rc<ObjectSlot>) -> Option<Box<dyn Any>> {
        let removed = self
            .objects
            .remove(&raw)
            .expect("object slot disappeared after lookup");
        debug_assert!(Rc::ptr_eq(&removed, &slot));
        drop(removed);
        if self.object_handles.contains(raw) {
            self.object_handles.retire(raw);
        }
        Some(
            Rc::try_unwrap(slot)
                .unwrap_or_else(|_| panic!("object slot still referenced after borrow release"))
                .into_object(),
        )
    }

    fn finish_deferred_object_drop(
        &mut self,
        handle: ObjectHandle,
        slot: Rc<ObjectSlot>,
    ) -> Option<Box<dyn Any>> {
        let raw = handle.raw();
        let current = self.objects.get(&raw)?;
        if !Rc::ptr_eq(current, &slot) || !current.pending_drop.get() {
            return None;
        }
        let Ok(value) = slot.value.try_borrow_mut() else {
            return None;
        };
        drop(value);
        self.remove_ready_object_slot(raw, slot)
    }
}

fn push_ref_list(buf: &mut Vec<u32>, refs: &[JsRef]) {
    buf.push(refs.len() as u32);
    for js_ref in refs {
        let id = js_ref.raw();
        buf.push((id & 0xFFFF_FFFF) as u32);
        buf.push((id >> 32) as u32);
    }
}

/// Operations the encoding layer (in the [`wire`](crate::wire) module) and the
/// semantic boundary (in `wry-bindgen-core`) reach for on the active runtime.
impl Runtime {
    pub fn resolve_function(&mut self, spec: JsFunctionSpec) -> u32 {
        crate::function_registry::FUNCTION_REGISTRY
            .resolve_function(spec)
            .unwrap_or_else(|| panic!("Function not found for code: {}", spec.render_js_code()))
    }

    pub fn insert_object_box(&mut self, obj: Box<dyn Any>) -> ObjectHandle {
        let handle = self.object_handles.alloc();
        self.objects.insert(handle, Rc::new(ObjectSlot::new(obj)));
        ObjectHandle::from_raw(handle)
    }

    pub fn take_thread_local_box<K>(&mut self, id_pointer: *const K) -> Option<Box<dyn Any>> {
        self.thread_locals.remove(&id_pointer.cast::<()>())
    }

    pub fn insert_thread_local_box<K>(&mut self, id_pointer: *const K, value: Box<dyn Any>) {
        self.thread_locals.insert(id_pointer.cast::<()>(), value);
    }

    pub(crate) fn get_next_inbound_js_ref(&mut self) -> JsRef {
        JsRef::from_raw(self.get_next_inbound_js_heap_id())
    }

    /// Reserve the next return-value placeholder as a [`JsRef`].
    ///
    /// Batched calls reserve the heap slot here so the typed result can be
    /// produced without a round-trip; JS fills the slot on the next flush.
    pub fn next_placeholder_ref(&mut self) -> JsRef {
        JsRef::from_raw(self.get_next_placeholder_id())
    }

    /// Reserve the next borrowed reference as a [`JsRef`].
    ///
    /// Borrowed references occupy the borrow stack (indices 1-127) rather than a
    /// heap slot; JS puts the value on its borrow stack without sending an id, so
    /// Rust syncs by taking the next borrow ref here.
    pub fn next_borrowed_ref(&mut self) -> JsRef {
        JsRef::from_raw(self.get_next_borrow_id())
    }
}

thread_local! {
    static RUNTIME: RefCell<Vec<Runtime>> = const { RefCell::new(Vec::new()) };
}

/// Install `runtime` as the active runtime for the duration of `run`, returning
/// it afterward. Nested calls stack, so re-entrant work sees the same runtime.
pub(crate) fn in_runtime<O>(runtime: Runtime, run: impl FnOnce() -> O) -> (Runtime, O) {
    RUNTIME.with(|state| state.borrow_mut().push(runtime));
    let out = run();
    let runtime = RUNTIME.with(|state| {
        state
            .borrow_mut()
            .pop()
            .expect("No runtime available to pop")
    });
    (runtime, out)
}

/// Run `f` with the active runtime. Panics if none is installed.
pub fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|state| {
        let mut state = state.borrow_mut();
        f(state.last_mut().expect("No runtime available"))
    })
}

/// Run `f` with the active runtime, or return `None` if none is installed or it
/// is already borrowed — the non-panicking path for `Drop` impls during
/// teardown.
fn try_with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> Option<R> {
    RUNTIME
        .try_with(|state| {
            let mut state = state.try_borrow_mut().ok()?;
            Some(f(state.last_mut()?))
        })
        .ok()
        .flatten()
}

fn finish_deferred_object_drop(handle: ObjectHandle, slot: Rc<ObjectSlot>) {
    if !slot.pending_drop.get() {
        return;
    }

    let object =
        try_with_runtime(|runtime| runtime.finish_deferred_object_drop(handle, slot)).flatten();
    drop(object);
}

fn runtime_installed() -> bool {
    RUNTIME
        .try_with(|state| {
            state
                .try_borrow()
                .map(|state| !state.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Release a JS heap reference, notifying JS now if no operation is open to
/// batch the drop into. A no-op if no runtime is installed (teardown).
pub(crate) fn drop_js_object(js_ref: JsRef) {
    let Some(Some(id)) = try_with_runtime(|runtime| runtime.release_heap_id(js_ref.raw())) else {
        return;
    };
    let js_ref = JsRef::from_raw(id);
    crate::js_helpers::js_drop_heap_ref(js_ref.raw());
    recycle_heap_id_after_js_drop(js_ref);
}

/// Dispose the JS wrapper for a Rust callback. A no-op if no runtime is
/// installed (teardown).
pub(crate) fn dispose_js_rust_function(js_ref: JsRef) {
    if runtime_installed() {
        crate::js_helpers::js_dispose_rust_function(js_ref.raw());
    }
}

/// Release a stored Rust object. A no-op if no runtime is installed (teardown).
pub(crate) fn drop_rust_object(handle: ObjectHandle) {
    let Some(object) = try_with_runtime(|runtime| runtime.release_object_handle(handle)) else {
        return;
    };
    drop(object);
}

/// Check if we're currently inside a batch() call
pub(crate) fn is_batching() -> bool {
    with_runtime(|state| state.is_batching())
}

fn recycle_heap_id_after_js_drop(js_ref: JsRef) {
    with_runtime(|runtime| {
        if runtime.is_batching() {
            runtime.defer_heap_id_recycle_until_flush(js_ref.raw());
        } else {
            runtime.recycle_heap_id(js_ref.raw());
        }
    });
}

/// Add an operation to the current batch.
pub(crate) fn add_operation(
    encoder: &mut EncodedData,
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData),
) -> bool {
    AbiBinaryEncode::encode(fn_id, encoder);
    add_args(encoder);
    encoder.take_needs_flush()
}

/// Pops the operation frame on drop unless disarmed, so a panic mid-flush (a
/// non-`catch` JS import threw) leaves the operation-frame stack balanced for
/// the destructors that run during the unwind.
struct OperationFrameGuard {
    armed: bool,
}

impl OperationFrameGuard {
    fn new() -> Self {
        with_runtime(|state| state.push_operation_frame());
        Self { armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for OperationFrameGuard {
    fn drop(&mut self) {
        if self.armed {
            with_runtime(|state| {
                state.pop_operation_frame();
            });
        }
    }
}

/// Run one JS operation synchronously and decode its typed result.
///
/// `add_args` encodes the call arguments. `reserve_placeholder` may return the
/// result without a round-trip (for batched calls whose value JS reserves ahead
/// of time); otherwise `decode_result` decodes the flushed response.
pub fn run_js_sync<R>(
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData),
    reserve_placeholder: impl FnOnce(&mut Runtime) -> Option<R>,
    mut decode_result: impl for<'a> FnMut(&mut DecodedData<'a>) -> R,
) -> R {
    // The guard pops this operation frame if the flush below panics (e.g. a
    // non-`catch` JS import threw and Rust must unwind), keeping the frame stack
    // balanced. The success path disarms it and drains the frame itself.
    let mut frame_guard = OperationFrameGuard::new();

    let mut batch = EncodedData::default();
    let needs_flush = add_operation(&mut batch, fn_id, add_args);
    with_runtime(|state| state.extend_encoder(batch));

    // A `&mut [T]` argument needs the JS side's response (the mutated arrays
    // travel back appended to it), so a call with pending write-backs must
    // decode the real response rather than short-circuit on a reserved
    // placeholder — and therefore must flush now rather than batch.
    let pending_write_backs = has_pending_write_backs();

    let mut placeholder = if pending_write_backs {
        None
    } else {
        with_runtime(|state| reserve_placeholder(state))
    };

    // Decode the return value, then apply any `&mut [T]` write-backs the
    // argument encoders queued: the JS side appends the mutated arrays after the
    // return value, so they are read from the same decoder once `R` is decoded.
    let mut decode_with_write_backs = move |mut data: DecodedData<'_>| {
        let value = decode_result(&mut data);
        run_write_backs(&mut data);
        value
    };

    let result = if !is_batching() || needs_flush || pending_write_backs {
        flush_and_then(move |data| {
            placeholder
                .take()
                .unwrap_or_else(|| decode_with_write_backs(data))
        })
    } else {
        placeholder.unwrap_or_else(|| flush_and_then(decode_with_write_backs))
    };

    frame_guard.disarm();
    let frame = with_runtime(|state| state.pop_operation_frame());
    for id in frame.heap_ids {
        let js_ref = JsRef::from_raw(id);
        crate::js_helpers::js_drop_heap_ref(js_ref.raw());
        recycle_heap_id_after_js_drop(js_ref);
    }
    for handle in frame.object_handles {
        let object = with_runtime(|state| state.drop_object_handle(ObjectHandle::from_raw(handle)));
        drop(object);
    }

    result
}

/// Flush the current batch and return the decoded result.
pub(crate) fn flush_and_return<R: BinaryDecode>() -> R {
    flush_and_then(|mut data| R::decode(&mut data).expect("Failed to decode return value"))
}

pub(crate) fn flush_and_then<R>(mut then: impl for<'a> FnMut(DecodedData<'a>) -> R) -> R {
    let (batch_msg, heap_ids_to_recycle_after_flush) = with_runtime(|state| state.take_message());

    // Send and wait for the matching Respond. Under strict ping-pong the next
    // non-Evaluate inbound is necessarily the answer to this outbound.
    with_runtime(|runtime| runtime.ipc().send_ipc(batch_msg));
    let mut heap_ids_to_recycle_after_flush = Some(heap_ids_to_recycle_after_flush);
    loop {
        if let Some(result) = crate::runtime::progress_js_with(&mut then) {
            recycle_heap_ids_after_flush(
                heap_ids_to_recycle_after_flush
                    .take()
                    .expect("heap IDs should only be recycled once per flush"),
            );
            return result;
        }
    }
}

fn recycle_heap_ids_after_flush(ids: Vec<JsRef>) {
    for id in ids {
        with_runtime(|state| {
            state.recycle_heap_id_if_released(id.raw());
        });
    }
}

pub fn batch<R, F: FnOnce() -> R>(f: F) -> R {
    let previous = with_runtime(|runtime| {
        let previous = runtime.is_batching();
        runtime.set_batching(true);
        previous
    });
    let result = f();
    if !previous {
        force_flush();
    }
    with_runtime(|runtime| runtime.set_batching(previous));
    result
}

pub fn batch_async<'a, R, F: core::future::Future<Output = R> + 'a>(
    f: F,
) -> impl core::future::Future<Output = R> + 'a {
    let mut f = Box::pin(f);
    std::future::poll_fn(move |ctx| batch(|| f.as_mut().poll(ctx)))
}

/// Force a flush of the current batch, even if we're inside a batch() call
pub fn force_flush() {
    let has_pending = with_runtime(|state| !state.is_empty());
    if has_pending {
        flush_and_return::<()>();
    }
}

#[cfg(test)]
mod take_encoder_tests {

    use super::*;
    use crate::ipc::DecodedVariant;
    use crate::runtime::WryIPC;

    fn test_runtime() -> Runtime {
        let (ipc, _senders, _driver_commands) = WryIPC::new();
        Runtime::new(ipc)
    }

    #[test]
    fn take_encoder_yields_an_evaluate_message_with_no_request_id() {
        let mut runtime = test_runtime();
        assert!(runtime.is_empty());

        let (message, _) = runtime.take_message();
        let DecodedVariant::Evaluate { .. } = message.decoded().unwrap() else {
            panic!("expected Evaluate message");
        };
        // The encoder holds only the single message-type byte — no per-message
        // request ID lives on the wire anymore.
    }

    #[test]
    fn object_shared_borrows_compose_and_mutable_borrow_conflicts() {
        let mut runtime = test_runtime();
        let handle = runtime.insert_object_box(Box::new(5_u32));

        let shared_a = runtime.object_ref::<u32>(handle).unwrap();
        let shared_b = runtime.object_ref::<u32>(handle).unwrap();
        assert_eq!(*shared_a, 5);
        assert_eq!(*shared_b, 5);
        match runtime.object_mut::<u32>(handle) {
            Err(ObjectBorrowError::RecursiveUse) => {}
            Err(error) => panic!("unexpected borrow error: {error:?}"),
            Ok(_) => panic!("mutable borrow should conflict with shared borrows"),
        }
        assert!(runtime.object_is::<u32>(handle));

        drop(shared_a);
        drop(shared_b);

        let mut exclusive = runtime.object_mut::<u32>(handle).unwrap();
        *exclusive = 9;
        assert_eq!(*exclusive, 9);
        drop(exclusive);

        assert_eq!(
            *runtime
                .remove_object_untyped(handle)
                .expect("valid handle")
                .downcast::<u32>()
                .expect("type mismatch"),
            9
        );
    }

    #[test]
    fn object_ownership_take_fails_while_borrowed_without_freeing_handle() {
        let mut runtime = test_runtime();
        let handle = runtime.insert_object_box(Box::new(5_u32));
        let handle_raw = handle.raw();

        let shared = runtime.object_ref::<u32>(handle).unwrap();
        assert_eq!(
            runtime.remove_object_untyped(handle).unwrap_err(),
            ObjectTakeError::Borrowed
        );
        assert!(runtime.object_is::<u32>(handle));
        drop(shared);

        assert_eq!(
            *runtime
                .remove_object_untyped(handle)
                .expect("valid handle")
                .downcast::<u32>()
                .expect("type mismatch"),
            5
        );

        let next = runtime.insert_object_box(Box::new(9_u32));
        assert_ne!(
            next.raw(),
            handle_raw,
            "object handles exposed to JS must not be reused; stale finalizers only carry the raw handle"
        );
    }

    #[test]
    fn object_drop_during_borrow_is_deferred_then_honored() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct DropFlag(Rc<Cell<bool>>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let runtime = test_runtime();
        let dropped = Rc::new(Cell::new(false));
        let dropped_for_runtime = dropped.clone();

        let (mut runtime, handle_raw) = in_runtime(runtime, move || {
            let handle =
                with_runtime(|rt| rt.insert_object_box(Box::new(DropFlag(dropped_for_runtime))));
            let borrowed = with_runtime(|rt| rt.object_ref::<DropFlag>(handle).unwrap());

            let object = with_runtime(|rt| rt.drop_object_handle(handle));
            assert!(object.is_none(), "borrowed object drop must be deferred");
            assert!(
                !dropped.get(),
                "object must not be dropped while it is borrowed"
            );
            assert!(
                !with_runtime(|rt| rt.object_is::<DropFlag>(handle)),
                "a deferred-drop handle should no longer be borrowable"
            );

            drop(borrowed);
            assert!(
                dropped.get(),
                "deferred drop must run once the last borrow releases"
            );
            handle.raw()
        });

        let next = runtime.insert_object_box(Box::new(DropFlag(Rc::new(Cell::new(false)))));
        assert_ne!(
            next.raw(),
            handle_raw,
            "deferred-drop handles must be retired once the drop completes"
        );
    }

    #[test]
    fn object_drop_waits_for_all_shared_borrows() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct DropFlag(Rc<Cell<u32>>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let runtime = test_runtime();
        let dropped = Rc::new(Cell::new(0));
        let dropped_for_runtime = dropped.clone();

        let (mut runtime, handle_raw) = in_runtime(runtime, move || {
            let handle =
                with_runtime(|rt| rt.insert_object_box(Box::new(DropFlag(dropped_for_runtime))));
            let borrowed_a = with_runtime(|rt| rt.object_ref::<DropFlag>(handle).unwrap());
            let borrowed_b = with_runtime(|rt| rt.object_ref::<DropFlag>(handle).unwrap());

            let object = with_runtime(|rt| rt.drop_object_handle(handle));
            assert!(object.is_none(), "borrowed object drop must be deferred");

            drop(borrowed_a);
            assert_eq!(
                dropped.get(),
                0,
                "object must remain alive while another shared borrow exists"
            );

            drop(borrowed_b);
            assert_eq!(
                dropped.get(),
                1,
                "deferred drop must run after the last shared borrow releases"
            );
            handle.raw()
        });

        let next = runtime.insert_object_box(Box::new(DropFlag(Rc::new(Cell::new(0)))));
        assert_ne!(
            next.raw(),
            handle_raw,
            "deferred-drop handles must be retired once the last borrow releases"
        );
    }
}
