//! Batching system for grouping multiple JS operations into single messages.
//!
//! This module provides the batching infrastructure that allows multiple
//! JS operations to be grouped together for efficient execution.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::any::Any;
use core::cell::RefCell;
use std::boxed::Box;

use crate::encode::{BatchableResult, BinaryDecode};
use crate::id_allocator::{BorrowIds, HeapIds, IdSlab, InstallIdBatch};
use crate::ipc::DecodedData;
use crate::ipc::{EncodedData, MessageType, OutboundIPCMessage};
use crate::lazy::ThreadLocalKey;
use crate::object_store::ObjectHandle;
use crate::runtime::WryIPC;
use crate::type_cache::TypeCache;
use crate::value::JSIDX_RESERVED;

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
    /// The encoder accumulating batched operations
    encoder: EncodedData,
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
    /// The ipc layer used to communicate with the JS runtime
    ipc: WryIPC,
    /// The id of the webview this is associated with
    webview_id: u64,
    /// Thread locals associated with the runtime
    thread_locals: BTreeMap<ThreadLocalKey<'static>, Box<dyn Any>>,
}

impl Runtime {
    pub(crate) fn new(ipc: WryIPC, webview_id: u64) -> Self {
        let encoder = Self::new_encoder_for_evaluate();
        Self {
            encoder,
            heap_ids: HeapIds::new(),
            borrow_ids: BorrowIds::new(),
            object_handles: IdSlab::new(1),
            is_batching: false,
            type_cache: TypeCache::new(),
            // Object store starts empty
            objects: BTreeMap::new(),
            pending_object_drops: BTreeSet::new(),
            op_free_stack: Vec::new(),
            ipc,
            webview_id,
            thread_locals: BTreeMap::new(),
        }
    }

    fn new_encoder_for_evaluate() -> EncodedData {
        let mut encoder = EncodedData::new();
        encoder.push_u8(MessageType::Evaluate as u8);
        encoder
    }

    /// Get a reference to the IPC layer.
    pub(crate) fn ipc(&self) -> &WryIPC {
        &self.ipc
    }

    /// Get the webview ID associated with this runtime.
    pub(crate) fn webview_id(&self) -> u64 {
        self.webview_id
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
        self.encoder.defer_heap_id_recycle_until_flush(id);
    }

    /// Take the message data and reset the batch for reuse.
    /// Includes ID installation and placeholder reservation metadata at the start of the message.
    pub(crate) fn take_message(&mut self) -> (OutboundIPCMessage, Vec<u64>) {
        let reserved_ids = self.take_reserved_placeholder_ids();
        let mut encoder = self.take_encoder();
        let heap_ids_to_recycle_after_flush = encoder.take_heap_ids_to_recycle_after_flush();
        (
            self.finish_rust_to_js_message(encoder, Some(&reserved_ids)),
            heap_ids_to_recycle_after_flush,
        )
    }

    /// Add Rust-to-JS response metadata and turn the encoder into a response message.
    pub(crate) fn finish_respond_message(&mut self, encoder: EncodedData) -> OutboundIPCMessage {
        self.finish_rust_to_js_message(encoder, None)
    }

    fn finish_rust_to_js_message(
        &mut self,
        mut encoder: EncodedData,
        reserved_ids: Option<&[u64]>,
    ) -> OutboundIPCMessage {
        let install_ids = self.take_pending_install_ids();
        prepend_rust_to_js_prelude(&mut encoder, &install_ids, reserved_ids);
        let pending_type_ids = encoder.take_pending_type_ids();
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
        OutboundIPCMessage::new(crate::ipc::IPCMessage::new(encoder.to_bytes()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        // 12 bytes for offsets, 1 byte for message type, and one u32 header word.
        self.encoder.byte_len() <= 17
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
            None => self.remove_object_untyped(handle.raw()),
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

    pub(crate) fn take_encoder(&mut self) -> EncodedData {
        let next = Self::new_encoder_for_evaluate();
        core::mem::replace(&mut self.encoder, next)
    }

    pub(crate) fn extend_encoder(&mut self, other: &EncodedData) {
        // Manually extend to avoid adding an extra message type byte for the
        // inner encoder.
        self.encoder.u8_buf.extend_from_slice(&other.u8_buf[1..]);
        self.encoder.u32_buf.extend_from_slice(&other.u32_buf);
        self.encoder.u16_buf.extend_from_slice(&other.u16_buf);
        self.encoder.str_buf.extend_from_slice(&other.str_buf);
        self.encoder
            .heap_ids_to_recycle_after_flush
            .extend_from_slice(&other.heap_ids_to_recycle_after_flush);
        self.encoder
            .pending_type_ids
            .extend_from_slice(&other.pending_type_ids);
        self.encoder.needs_flush |= other.needs_flush;
    }

    /// Get or create a type ID for a function-type definition. The second
    /// element is true if JS has already acked a `TYPE_FULL` for this ID.
    pub(crate) fn get_or_create_type_id(&mut self, type_bytes: &[u8]) -> (u32, bool) {
        self.type_cache.get_or_create_type_id(type_bytes)
    }

    /// Pop the top pending-ack frame and mark its type IDs as acked. Called
    /// when an inbound JS Respond arrives.
    pub(crate) fn pop_and_ack_type_cache_frame(&mut self) {
        self.type_cache.pop_and_ack_pending_frame();
    }

    /// Insert an exported object and return its handle.
    pub(crate) fn insert_object<T: 'static>(&mut self, obj: T) -> u32 {
        let handle = self.object_handles.alloc();
        self.objects.insert(handle, Box::new(obj));
        handle
    }

    /// Get a thread-local variable.
    pub(crate) fn take_thread_local<T: 'static>(&mut self, key: ThreadLocalKey<'static>) -> T {
        *self
            .thread_locals
            .remove(&key)
            .expect("thread local not found")
            .downcast::<T>()
            .expect("type mismatch")
    }

    /// Insert a thread-local variable.
    pub(crate) fn insert_thread_local<T: 'static>(
        &mut self,
        key: ThreadLocalKey<'static>,
        value: T,
    ) {
        self.thread_locals.insert(key, Box::new(value));
    }

    /// Check if a thread-local variable exists.
    pub(crate) fn has_thread_local(&self, key: ThreadLocalKey<'static>) -> bool {
        self.thread_locals.contains_key(&key)
    }

    pub(crate) fn get_object<T: 'static>(&self, handle: u32) -> &T {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        boxed.downcast_ref::<T>().expect("type mismatch")
    }

    pub(crate) fn take_object<T: 'static>(&mut self, handle: u32) -> T {
        let boxed = self.objects.remove(&handle).expect("invalid handle");
        *boxed.downcast::<T>().expect("type mismatch")
    }

    pub(crate) fn reinsert_object<T: 'static>(&mut self, handle: u32, obj: T) {
        // A drop (e.g. JS GC firing DROP_NATIVE_REF) may have arrived while this
        // object was checked out. Honor it now instead of resurrecting the
        // object: free the handle and let `obj` drop.
        if self.pending_object_drops.remove(&handle) {
            self.object_handles.free(handle);
            drop(obj);
            return;
        }
        assert!(
            self.objects.insert(handle, Box::new(obj)).is_none(),
            "object handle {handle} was reinserted while occupied"
        );
    }

    /// Remove an exported object and return it.
    pub(crate) fn remove_object<T: 'static>(&mut self, handle: u32) -> T {
        let boxed = self.objects.remove(&handle).expect("invalid handle");
        self.object_handles.free(handle);
        *boxed.downcast::<T>().expect("type mismatch")
    }

    pub(crate) fn remove_object_untyped(&mut self, handle: u32) -> Option<Box<dyn Any>> {
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

fn push_id_list(buf: &mut Vec<u32>, ids: &[u64]) {
    buf.push(ids.len() as u32);
    for &id in ids {
        buf.push((id & 0xFFFF_FFFF) as u32);
        buf.push((id >> 32) as u32);
    }
}

fn prepend_rust_to_js_prelude(
    encoder: &mut EncodedData,
    install_ids: &[u64],
    reserved_ids: Option<&[u64]>,
) {
    let mut prelude = Vec::new();
    // A single install id-list: empty (count 0) when the last inbound message
    // carried no heap refs, otherwise the IDs for that one batch.
    push_id_list(&mut prelude, install_ids);
    if let Some(reserved_ids) = reserved_ids {
        push_id_list(&mut prelude, reserved_ids);
    }
    // The message type lives in the u8 buffer, so the u32 buffer starts with
    // the prelude at index 0 (no request_id word precedes it anymore).
    encoder.insert_u32s(0, &prelude);
}

thread_local! {
    /// Thread-local runtime state - always exists, reset after each flush
    pub(crate) static RUNTIME: RefCell<Vec<Runtime>> = const { RefCell::new(Vec::new()) };
}

fn push_runtime(runtime: Runtime) {
    RUNTIME.with(|state| {
        state.borrow_mut().push(runtime);
    });
}

fn pop_runtime() -> Runtime {
    RUNTIME.with(|state| {
        state
            .borrow_mut()
            .pop()
            .expect("No runtime available to pop")
    })
}

pub(crate) fn in_runtime<O>(runtime: Runtime, run: impl FnOnce() -> O) -> (Runtime, O) {
    push_runtime(runtime);
    let out = run();
    let runtime = pop_runtime();
    (runtime, out)
}

pub(crate) fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|state| {
        let mut state = state.borrow_mut();
        f(state.last_mut().expect("No runtime available"))
    })
}

/// Check if we're currently inside a batch() call
pub(crate) fn is_batching() -> bool {
    with_runtime(|state| state.is_batching())
}

/// Whether the runtime is unavailable — already dropped, inaccessible, or
/// borrowed elsewhere. Callers treat all of these the same: skip the work.
fn runtime_already_dropped() -> bool {
    match RUNTIME.try_with(|state| {
        state
            .try_borrow()
            .map(|runtime_stack| runtime_stack.is_empty())
    }) {
        Ok(Ok(value)) => value,
        Ok(Err(_)) | Err(_) => true,
    }
}

/// Queue a JS drop operation for a heap ID.
/// This is called when a JsValue is dropped.
pub(crate) fn queue_js_drop(id: u64) {
    debug_assert!(
        id >= JSIDX_RESERVED,
        "Attempted to drop reserved JS heap ID {id}"
    );

    if runtime_already_dropped() {
        return;
    }

    let id = match RUNTIME.try_with(|state| {
        state.try_borrow_mut().ok().and_then(|mut runtime_stack| {
            runtime_stack
                .last_mut()
                .map(|runtime| runtime.release_heap_id(id))
        })
    }) {
        Ok(Some(id)) => id,
        Ok(None) | Err(_) => return,
    };
    if let Some(id) = id {
        crate::js_helpers::js_drop_heap_ref(id);
        recycle_heap_id_after_js_drop(id);
    }
}

/// Mark the RustFunction wrapper at this heap ID as disposed. The heap-ref
/// release is the responsibility of the caller — typically `JsValue::drop`
/// running immediately after this via field-drop glue on `ScopedClosure`.
pub(crate) fn queue_js_dispose_rust_function(id: u64) {
    debug_assert!(
        id >= JSIDX_RESERVED,
        "Attempted to dispose reserved JS heap ID {id}"
    );

    if runtime_already_dropped() {
        return;
    }

    crate::js_helpers::js_dispose_rust_function(id);
}

fn recycle_heap_id_after_js_drop(id: u64) {
    let _ = RUNTIME.try_with(|state| {
        let Ok(mut runtime_stack) = state.try_borrow_mut() else {
            return;
        };
        let Some(runtime) = runtime_stack.last_mut() else {
            return;
        };

        if runtime.is_batching() {
            runtime.defer_heap_id_recycle_until_flush(id);
        } else {
            runtime.recycle_heap_id(id);
        }
    });
}

/// Drop a Rust-owned object now, or after the current encoded JS operation
/// finishes if that object is being passed to JS.
pub(crate) fn queue_rust_object_drop(handle: ObjectHandle) {
    let object = RUNTIME
        .try_with(|state| {
            state.try_borrow_mut().ok().and_then(|mut runtime_stack| {
                runtime_stack
                    .last_mut()
                    .and_then(|runtime| runtime.release_object_handle(handle))
            })
        })
        .unwrap_or_default();
    drop(object);
}

/// Add an operation to the current batch.
pub(crate) fn add_operation(
    encoder: &mut EncodedData,
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData),
) {
    encoder.push_u32(fn_id);
    add_args(encoder);
}

/// Core function for executing JavaScript calls.
///
/// For each call:
/// 1. Encode the current evaluate message into the current batch
/// 2. If the return value is needed immediately, flush the batch and return the result
/// 3. Otherwise get the pending result from BatchableResult
pub(crate) fn run_js_sync<R: BatchableResult>(
    fn_id: u32,
    add_args: impl FnOnce(&mut EncodedData),
) -> R {
    // Step 1: Encode the operation into the batch and get placeholder for non-flush types
    // We take the current encoder out of the thread-local state to avoid borrowing issues
    // and then put it back after adding the operation. Drops or other calls may happen while
    // we are encoding, but they should be queued after this operation.
    let mut batch = with_runtime(|state| {
        // Push a new operation into the batch
        state.push_operation_frame();
        state.take_encoder()
    });
    add_operation(&mut batch, fn_id, add_args);

    // Check if any encoded argument requires immediate flush (e.g., stack-allocated callbacks)
    let needs_flush = batch.needs_flush;

    with_runtime(|state| {
        let encoded_during_op = core::mem::replace(&mut state.encoder, batch);
        state.extend_encoder(&encoded_during_op);
    });

    // Reserve placeholders before any flush so JS receives exact IDs to fill.
    let mut placeholder = with_runtime(|state| R::try_placeholder(state));

    // Must flush if: not batching, or if the operation requires immediate execution
    // (e.g., stack-allocated callbacks that must be invoked before returning)
    let result = if !is_batching() || needs_flush {
        flush_and_then(move |mut data| {
            let response = placeholder
                .take()
                .unwrap_or_else(|| R::decode(&mut data).expect("Failed to decode return value"));
            assert!(
                data.is_empty(),
                "Extra data remaining after decoding response"
            );
            response
        })
    } else {
        placeholder.unwrap_or_else(|| flush_and_return::<R>())
    };

    // After running, free any queued IDs and object handles for this operation
    let frame = with_runtime(|state| state.pop_operation_frame());
    for id in frame.heap_ids {
        crate::js_helpers::js_drop_heap_ref(id);
        recycle_heap_id_after_js_drop(id);
    }
    for handle in frame.object_handles {
        let object = with_runtime(|state| state.remove_object_untyped(handle));
        drop(object);
    }

    result
}

/// Flush the current batch and return the decoded result.
pub(crate) fn flush_and_return<R: BinaryDecode>() -> R {
    flush_and_then(|mut data| {
        let response = R::decode(&mut data).expect("Failed to decode return value");
        assert!(
            data.is_empty(),
            "Extra data remaining after decoding response"
        );
        response
    })
}

pub(crate) fn flush_and_then<R>(mut then: impl for<'a> FnMut(DecodedData<'a>) -> R) -> R {
    use crate::runtime::WryBindgenEvent;

    let (batch_msg, heap_ids_to_recycle_after_flush) = with_runtime(|state| state.take_message());

    // Send and wait for the matching Respond. Under strict ping-pong the next
    // non-Evaluate inbound is necessarily the answer to this outbound.
    with_runtime(|runtime| {
        (runtime.ipc().proxy)(WryBindgenEvent::ipc(runtime.webview_id(), batch_msg))
    });
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

fn recycle_heap_ids_after_flush(ids: Vec<u64>) {
    for id in ids {
        with_runtime(|state| {
            state.recycle_heap_id_if_released(id);
        });
    }
}

/// Execute operations inside a batch. Operations that return opaque types (like JsValue)
/// will be batched and executed together. Operations that return non-opaque types will
/// flush the batch to get the actual result.
pub fn batch<R, F: FnOnce() -> R>(f: F) -> R {
    let currently_batching = is_batching();
    // Start batching
    with_runtime(|state| state.set_batching(true));

    // Execute the closure
    let result = f();

    if !currently_batching {
        // Flush any remaining batched operations
        force_flush();
    }

    // End batching
    with_runtime(|state| state.set_batching(currently_batching));

    result
}

/// Like `batch`, but async.
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
    use std::sync::Arc;

    use super::*;
    use crate::ipc::{DecodedVariant, IPCMessage};
    use crate::runtime::WryIPC;

    fn test_runtime() -> Runtime {
        let (ipc, _senders) = WryIPC::new(Arc::new(|_| {}));
        Runtime::new(ipc, 0)
    }

    #[test]
    fn take_encoder_yields_an_evaluate_message_with_no_request_id() {
        let mut runtime = test_runtime();
        assert!(runtime.is_empty());

        let first = runtime.take_encoder();
        let bytes = IPCMessage::new(first.to_bytes());
        assert!(matches!(
            bytes.decoded().unwrap(),
            DecodedVariant::Evaluate { .. }
        ));
        // The encoder holds only the single message-type byte — no per-message
        // request ID lives on the wire anymore.
        assert!(first.u32_buf.is_empty());
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
        let handle = runtime.insert_object(DropFlag(dropped.clone()));

        // `with_object`/`with_object_mut` take the object out of the map for the
        // duration of a method call, leaving the handle live but the slot empty.
        let checked_out = runtime.take_object::<DropFlag>(handle);

        // A drop arrives mid-call (e.g. JS GC fires DROP_NATIVE_REF during a
        // nested callback). It must be deferred, not silently lost.
        assert!(runtime.remove_object_untyped(handle).is_none());
        assert!(
            !dropped.get(),
            "object must not be dropped while it is checked out"
        );

        // Finishing the checkout honors the deferred drop instead of
        // resurrecting the object.
        runtime.reinsert_object(handle, checked_out);
        assert!(
            dropped.get(),
            "deferred drop must run once the checkout completes"
        );

        // The handle was freed, so the next allocation reuses it (no leak).
        let reused = runtime.insert_object(DropFlag(Rc::new(Cell::new(false))));
        assert_eq!(reused, handle);
    }
}
