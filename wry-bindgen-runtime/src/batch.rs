//! Batching system for grouping multiple JS operations into single messages.
//!
//! This module provides the batching infrastructure that allows multiple
//! JS operations to be grouped together for efficient execution.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::any::Any;
use core::cell::RefCell;
use std::boxed::Box;
use std::thread_local;

use crate::encode::BinaryDecode;
use crate::id_allocator::{BorrowIds, HeapIds, IdSlab, InstallIdBatch};
use crate::ipc::DecodedData;
use crate::ipc::{EncodedData, EncodedParts, IPCMessage, MessageType};
use crate::object_store::ObjectHandle;
use crate::runtime::WryIPC;
use crate::type_cache::TypeCache;
use wry_bindgen_abi::BinaryEncode as AbiBinaryEncode;
use wry_bindgen_abi::unstable::{JsFunctionSpecRuntime, JsRefRaw, ObjectHandleRaw};
use wry_bindgen_abi::{JsFunctionSpec, JsRef, TypeDef};

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
    objects: BTreeMap<u32, Box<dyn Any>>,
    /// Handles whose drop arrived while the object was checked out (taken out of
    /// `objects` for a `with_object`/`with_object_mut` call). The drop is honored
    /// when the checkout finishes instead of being lost.
    pending_object_drops: BTreeSet<u32>,
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
<<<<<<< /tmp/ours_batch.rs
    thread_locals: BTreeMap<*const (), Box<dyn Any>>,
||||||| /tmp/base_batch.rs
    thread_locals: BTreeMap<ThreadLocalKey<'static>, Box<dyn Any>>,
    /// How many JS→Rust callbacks (inbound Evaluates) are currently executing
    /// on the stack. Zero means any outbound Evaluate is a fresh top-level call
    /// from the app future; non-zero means it is a nested response inside a
    /// callback and must travel back through the parked JS XHR.
    inbound_evaluate_depth: u32,
=======
    thread_locals: BTreeMap<ThreadLocalKey<'static>, Box<dyn Any>>,
>>>>>>> /tmp/theirs_batch.rs
}

impl Runtime {
<<<<<<< /tmp/ours_batch.rs
    pub(crate) fn new(ipc: WryIPC) -> Self {
||||||| /tmp/base_batch.rs
    pub(crate) fn new(ipc: WryIPC, webview_id: u64) -> Self {
        let encoder = Self::new_encoder_for_evaluate();
=======
    pub(crate) fn new(ipc: WryIPC) -> Self {
        let encoder = Self::new_encoder_for_evaluate();
>>>>>>> /tmp/theirs_batch.rs
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
            pending_object_drops: BTreeSet::new(),
            op_free_stack: Vec::new(),
            heap_ids_to_recycle_after_flush: Vec::new(),
            pending_type_ids: Vec::new(),
            ipc,
            thread_locals: BTreeMap::new(),
        }
    }

<<<<<<< /tmp/ours_batch.rs
    /// Get a reference to the IPC layer.
    pub(crate) fn ipc(&self) -> &WryIPC {
        &self.ipc
||||||| /tmp/base_batch.rs
    /// Mark that a JS→Rust callback (inbound Evaluate) has started executing.
    pub(crate) fn enter_inbound_evaluate(&mut self) {
        self.inbound_evaluate_depth += 1;
    }

    /// Mark that a JS→Rust callback has finished executing.
    pub(crate) fn leave_inbound_evaluate(&mut self) {
        self.inbound_evaluate_depth -= 1;
    }

    /// Whether we are currently executing inside a JS→Rust callback. When true,
    /// outbound Evaluates are nested responses to the parked JS XHR rather than
    /// fresh top-level calls.
    fn in_inbound_evaluate(&self) -> bool {
        self.inbound_evaluate_depth > 0
    }

    fn new_encoder_for_evaluate() -> EncodedData {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Evaluate as u8);
        encoder
    }

    /// Record a JS-allocated heap ID from a response.
    pub fn observe_js_heap_id(&mut self, id: u64) {
        self.heap_ids.observe_js_heap_id(id);
=======
    fn new_encoder_for_evaluate() -> EncodedData {
        let mut encoder = EncodedData::default();
        encoder.push_u8(MessageType::Evaluate as u8);
        encoder
    }

    /// Get a reference to the IPC layer.
    pub(crate) fn ipc(&self) -> &WryIPC {
        &self.ipc
>>>>>>> /tmp/theirs_batch.rs
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

<<<<<<< /tmp/ours_batch.rs
    pub(crate) fn defer_heap_id_recycle_until_flush(&mut self, id: u64) {
        self.heap_ids_to_recycle_after_flush
            .push(JsRef::from_raw(id));
||||||| /tmp/base_batch.rs
    pub fn defer_heap_id_recycle_until_flush(&mut self, id: u64) {
        self.encoder.defer_heap_id_recycle_until_flush(id);
=======
    pub(crate) fn defer_heap_id_recycle_until_flush(&mut self, id: u64) {
        self.encoder.defer_heap_id_recycle_until_flush(id);
>>>>>>> /tmp/theirs_batch.rs
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
<<<<<<< /tmp/ours_batch.rs
        encoder.into_message(message_type, &prelude)
||||||| /tmp/base_batch.rs
        // Only Evaluates (reserved_ids is Some) can be top-level; they are
        // top-level exactly when no callback is currently on the stack.
        let top_level = reserved_ids.is_some() && !self.in_inbound_evaluate();
        OutboundIPCMessage::new(crate::ipc::IPCMessage::new(encoder.to_bytes()), top_level)
=======
        OutboundIPCMessage::new(crate::ipc::IPCMessage::new(encoder.to_bytes()))
>>>>>>> /tmp/theirs_batch.rs
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
            None => self.remove_object_untyped(handle),
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

    pub(crate) fn get_object<T: 'static>(&self, handle: u32) -> &T {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        boxed.downcast_ref::<T>().expect("type mismatch")
    }

    pub fn remove_object_untyped(&mut self, handle: ObjectHandle) -> Option<Box<dyn Any>> {
        let handle = handle.raw();
        let object = self.objects.remove(&handle);
        if object.is_some() {
            self.object_handles.free(handle);
        } else if self.object_handles.contains(handle) {
            // The handle is live but absent from the map, so the object is
            // currently checked out by a `with_object`/`with_object_mut` call
            // (e.g. a method that triggered this drop through a nested
            // callback). Defer the drop until the checkout finishes; the
            // matching `reinsert_object` honors it.
            self.pending_object_drops.insert(handle);
        }
        object
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

/// Operations the encoding layer (in `wry-bindgen-abi`) and the semantic
/// boundary (in `wry-bindgen-core`) reach for on the active runtime.
impl Runtime {
    pub fn resolve_function(&mut self, spec: JsFunctionSpec) -> u32 {
        crate::function_registry::FUNCTION_REGISTRY
            .resolve_function(spec)
            .unwrap_or_else(|| panic!("Function not found for code: {}", spec.render_js_code()))
    }

    pub fn insert_object_box(&mut self, obj: Box<dyn Any>) -> ObjectHandle {
        let handle = self.object_handles.alloc();
        self.objects.insert(handle, obj);
        ObjectHandle::from_raw(handle)
    }

    pub fn take_object_box(&mut self, handle: ObjectHandle) -> Option<Box<dyn Any>> {
        self.objects.remove(&handle.raw())
    }

    pub fn reinsert_object_box(&mut self, handle: ObjectHandle, obj: Box<dyn Any>) {
        if self.pending_object_drops.remove(&handle.raw()) {
            self.object_handles.free(handle.raw());
            drop(obj);
            return;
        }
        assert!(
            self.objects.insert(handle.raw(), obj).is_none(),
            "object handle {} was reinserted while occupied",
            handle.raw()
        );
    }

    pub fn take_thread_local_box<K>(&mut self, key: &'static K) -> Option<Box<dyn Any>> {
        self.thread_locals
            .remove(&core::ptr::from_ref(key).cast::<()>())
    }

    pub fn insert_thread_local_box<K>(&mut self, key: &'static K, value: Box<dyn Any>) {
        self.thread_locals
            .insert(core::ptr::from_ref(key).cast::<()>(), value);
    }

    pub(crate) fn get_next_inbound_js_ref(&mut self) -> JsRef {
        JsRef::from_raw(self.get_next_inbound_js_heap_id())
    }
}

thread_local! {
    static RUNTIME: RefCell<Vec<Runtime>> = const { RefCell::new(Vec::new()) };
}

/// Install `runtime` as the active runtime for the duration of `run`, returning
/// it afterward. Nested calls stack, so re-entrant work sees the same runtime.
pub(crate) fn in_runtime<O>(runtime: Runtime, run: impl FnOnce() -> O) -> (Runtime, O) {
    install_hooks();
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

<<<<<<< /tmp/ours_batch.rs
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
||||||| /tmp/base_batch.rs
/// Check if we're currently inside a batch() call
pub fn is_batching() -> bool {
    with_runtime(|state| state.is_batching())
=======
/// Check if we're currently inside a batch() call
pub(crate) fn is_batching() -> bool {
    with_runtime(|state| state.is_batching())
>>>>>>> /tmp/theirs_batch.rs
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

fn hook_insert_object(obj: Box<dyn Any>) -> ObjectHandle {
    with_runtime(|runtime| runtime.insert_object_box(obj))
}

fn hook_queue_rust_object_drop(handle: ObjectHandle) {
    queue_rust_object_drop(handle);
}

fn hook_next_inbound_js_ref() -> JsRef {
    with_runtime(|runtime| runtime.get_next_inbound_js_ref())
}

static HOOKS: wry_bindgen_abi::unstable::RuntimeHooks = wry_bindgen_abi::unstable::RuntimeHooks {
    insert_object: hook_insert_object,
    queue_rust_object_drop: hook_queue_rust_object_drop,
    next_inbound_js_ref: hook_next_inbound_js_ref,
};

fn install_hooks() {
    wry_bindgen_abi::unstable::install_runtime_hooks(&HOOKS);
}

/// Queue a stored object for drop. Called while encoding a callback, where an
/// open operation frame defers the drop until the operation completes.
pub fn queue_rust_object_drop(handle: ObjectHandle) {
    let object = with_runtime(|runtime| runtime.release_object_handle(handle));
    drop(object);
}

/// Release a JS heap reference, notifying JS now if no operation is open to
/// batch the drop into. A no-op if no runtime is installed (teardown).
pub fn try_queue_js_drop(js_ref: JsRef) {
    let Some(Some(id)) = try_with_runtime(|runtime| runtime.release_heap_id(js_ref.raw())) else {
        return;
    };
    let js_ref = JsRef::from_raw(id);
    crate::js_helpers::js_drop_heap_ref(js_ref.raw());
    recycle_heap_id_after_js_drop(js_ref);
}

/// Dispose the JS-side wrapper for a Rust function. A no-op if no runtime is
/// installed (teardown).
pub fn try_queue_js_dispose_rust_function(js_ref: JsRef) {
    if runtime_installed() {
        crate::js_helpers::js_dispose_rust_function(js_ref.raw());
    }
}

/// Queue a stored object for drop. A no-op if no runtime is installed
/// (teardown).
pub fn try_queue_rust_object_drop(handle: ObjectHandle) {
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
    add_args: impl FnOnce(&mut EncodedData) -> bool,
) -> bool {
    AbiBinaryEncode::encode(fn_id, encoder);
    add_args(encoder)
}

/// Run one JS operation synchronously and decode its typed result.
///
/// `add_args` encodes the call arguments and returns whether the batch must
/// flush immediately. `reserve_placeholder` may return the result without a
/// round-trip (for batched calls whose value JS reserves ahead of time);
/// otherwise `decode_result` decodes the flushed response.
pub fn run_js_sync<R>(
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData) -> bool,
    reserve_placeholder: impl FnOnce(&mut Runtime) -> Option<R>,
    mut decode_result: impl for<'a> FnMut(DecodedData<'a>) -> R,
) -> R {
    with_runtime(|state| state.push_operation_frame());

    let mut batch = EncodedData::default();
    let needs_flush = add_operation(&mut batch, fn_id, add_args);
    with_runtime(|state| state.extend_encoder(batch));

    let mut placeholder = with_runtime(|state| reserve_placeholder(state));

    let result = if !is_batching() || needs_flush {
        flush_and_then(move |data| placeholder.take().unwrap_or_else(|| decode_result(data)))
    } else {
        placeholder.unwrap_or_else(|| flush_and_then(decode_result))
    };

    let frame = with_runtime(|state| state.pop_operation_frame());
    for id in frame.heap_ids {
        let js_ref = JsRef::from_raw(id);
        crate::js_helpers::js_drop_heap_ref(js_ref.raw());
        recycle_heap_id_after_js_drop(js_ref);
    }
    for handle in frame.object_handles {
        let object =
            with_runtime(|state| state.remove_object_untyped(ObjectHandle::from_raw(handle)));
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
<<<<<<< /tmp/ours_batch.rs
    use crate::ipc::DecodedVariant;
||||||| /tmp/base_batch.rs
    use crate::ipc::IPCMessage;
=======
    use crate::ipc::{DecodedVariant, IPCMessage};
>>>>>>> /tmp/theirs_batch.rs
    use crate::runtime::WryIPC;

    fn test_runtime() -> Runtime {
<<<<<<< /tmp/ours_batch.rs
        let (ipc, _senders) = WryIPC::new(Arc::new(|_| {}), 0);
        Runtime::new(ipc)
||||||| /tmp/base_batch.rs
        let (ipc, _senders) = WryIPC::new(Arc::new(|_| {}));
        Runtime::new(ipc, 0)
=======
        let (ipc, _senders, _driver_commands) = WryIPC::new();
        Runtime::new(ipc)
>>>>>>> /tmp/theirs_batch.rs
    }

    #[test]
    fn take_encoder_yields_an_evaluate_message_with_no_request_id() {
        let mut runtime = test_runtime();
        assert!(runtime.is_empty());

<<<<<<< /tmp/ours_batch.rs
        let (message, _) = runtime.take_message();
        let DecodedVariant::Evaluate { .. } = message.decoded().unwrap() else {
            panic!("expected Evaluate message");
        };
||||||| /tmp/base_batch.rs
        let first = runtime.take_encoder();
        let bytes = IPCMessage::new(first.to_bytes());
        assert_eq!(bytes.ty().unwrap(), MessageType::Evaluate);
=======
        let first = runtime.take_encoder();
        let bytes = IPCMessage::new(first.to_bytes());
        assert!(matches!(
            bytes.decoded().unwrap(),
            DecodedVariant::Evaluate { .. }
        ));
>>>>>>> /tmp/theirs_batch.rs
        // The encoder holds only the single message-type byte — no per-message
        // request ID lives on the wire anymore.
    }

    #[test]
    fn object_drop_during_checkout_is_deferred_then_honored() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct DropFlag(Rc<Cell<bool>>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let mut runtime = test_runtime();
        let dropped = Rc::new(Cell::new(false));
        let handle = runtime.insert_object_box(Box::new(DropFlag(dropped.clone())));

        // `with_object`/`with_object_mut` take the object out of the map for the
        // duration of a method call, leaving the handle live but the slot empty.
        let checked_out = runtime
            .take_object_box(handle)
            .expect("invalid handle")
            .downcast::<DropFlag>()
            .expect("type mismatch");

        // A drop arrives mid-call (e.g. JS GC fires DROP_NATIVE_REF during a
        // nested callback). It must be deferred, not silently lost.
        assert!(runtime.remove_object_untyped(handle).is_none());
        assert!(
            !dropped.get(),
            "object must not be dropped while it is checked out"
        );

        // Finishing the checkout honors the deferred drop instead of
        // resurrecting the object.
        runtime.reinsert_object_box(handle, checked_out);
        assert!(
            dropped.get(),
            "deferred drop must run once the checkout completes"
        );

        // The handle was freed, so the next allocation reuses it (no leak).
        let reused = runtime.insert_object_box(Box::new(DropFlag(Rc::new(Cell::new(false)))));
        assert_eq!(reused.raw(), handle.raw());
    }

    #[test]
    fn remove_frees_handle_unlike_checkout_take() {
        let mut runtime = test_runtime();

        // A checkout takes the object out of the map but leaves the handle
        // live, so its slot is not recycled.
        let checked_out = runtime.insert_object_box(Box::new(5_u32));
        let checked_out_raw = checked_out.raw();
        let value = runtime
            .take_object_box(checked_out)
            .expect("invalid handle");
        assert_eq!(value.downcast_ref::<u32>().copied(), Some(5));

        // A removal frees the handle for reuse.
        let removable = runtime.insert_object_box(Box::new(7_u32));
        let removable_raw = removable.raw();
        assert_ne!(removable_raw, checked_out_raw);
        assert_eq!(
            *runtime
                .remove_object_untyped(removable)
                .expect("invalid handle")
                .downcast::<u32>()
                .expect("type mismatch"),
            7
        );

        // The freed handle is reused; the checked-out one is not.
        let reused = runtime.insert_object_box(Box::new(9_u32));
        assert_eq!(reused.raw(), removable_raw);

        runtime.reinsert_object_box(checked_out, value);
    }
}
