//! Batching system for grouping multiple JS operations into single messages.
//!
//! This module provides the batching infrastructure that allows multiple
//! JS operations to be grouped together for efficient execution.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::any::Any;
use core::cell::{Ref, RefCell, RefMut};
use std::boxed::Box;

use crate::encode::{BatchableResult, BinaryDecode};
use crate::ipc::DecodedData;
use crate::ipc::{EncodedData, IPCMessage, MessageType};
use crate::lazy::ThreadLocalKey;
use crate::runtime::WryIPC;
use crate::value::{JSIDX_OFFSET, JSIDX_RESERVED};

/// State for batching operations and object storage.
/// Every evaluation is a batch - it may just have one operation.
///
/// Uses a free-list strategy for heap ID allocation to stay in sync with JS heap.
/// Also stores exported Rust structs and callback functions.
pub struct Runtime {
    /// The encoder accumulating batched operations
    encoder: EncodedData,
    /// Stack of freed IDs available for reuse
    free_ids: Vec<u64>,
    /// Next ID to allocate if free_ids is empty
    max_id: u64,
    /// A stack of ongoing function encodings with the ids
    /// that need to be freed after each one is done
    ids_to_free: Vec<Vec<u64>>,
    /// Whether we're inside a batch() call
    is_batching: bool,
    /// Borrow stack pointer - uses indices 1-127, growing downward from JSIDX_OFFSET (128) to 1
    /// Reset after each operation completes
    borrow_stack_pointer: u64,
    /// Frame stack for nested operations - saves borrow stack pointers
    borrow_frame_stack: Vec<u64>,
    /// Count of IDs reserved as placeholders during the current batch.
    /// This is sent to JS so it can skip these IDs during nested callback allocations.
    reserved_placeholder_count: u32,
    /// Type cache for avoiding resending type definitions to JS
    type_cache: BTreeMap<Vec<u8>, u32>,
    /// Next type ID to assign
    next_type_id: u32,
    /// Exported Rust structs stored by handle
    objects: BTreeMap<u32, Box<dyn Any>>,
    /// Next handle to assign for exported objects
    next_object_handle: u32,
    /// The ipc layer used to communicate with the JS runtime
    ipc: WryIPC,
    /// The id of the webview this is associated with
    webview_id: u64,
    /// Thread locals associated with the runtime
    thread_locals: BTreeMap<ThreadLocalKey<'static>, Box<dyn Any>>,
}

impl Runtime {
    pub(crate) fn new(ipc: WryIPC, webview_id: u64) -> Self {
        Self {
            encoder: Self::new_encoder_for_evaluate(),
            free_ids: Vec::new(),
            // Start allocating heap IDs from JSIDX_RESERVED to match JS heap
            max_id: JSIDX_RESERVED,
            ids_to_free: Vec::new(),
            is_batching: false,
            // Borrow stack starts at JSIDX_OFFSET (128) and grows downward to 1
            borrow_stack_pointer: JSIDX_OFFSET,
            // Frame stack starts empty
            borrow_frame_stack: Vec::new(),
            // No reserved placeholders initially
            reserved_placeholder_count: 0,
            // Type cache starts empty
            type_cache: BTreeMap::new(),
            // Type IDs start at 0
            next_type_id: 0,
            // Object store starts empty
            objects: BTreeMap::new(),
            // Object handles start at 0
            next_object_handle: 0,
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

    /// Get the next heap ID for placeholder allocation.
    /// Uses free-list strategy: reuses freed IDs first, then allocates new ones.
    pub fn get_next_heap_id(&mut self) -> u64 {
        let id = self.max_id;
        self.max_id += 1;
        id
    }

    /// Get the next heap ID for a batched return value placeholder.
    /// This also tracks the reserved placeholder count so JS can skip these IDs
    /// during nested callback allocations.
    pub fn get_next_placeholder_id(&mut self) -> u64 {
        let id = self.get_next_heap_id();
        if self.is_batching {
            self.reserved_placeholder_count += 1;
        }
        id
    }

    /// Get the next borrow ID from the borrow stack (indices 1-127).
    /// The borrow stack grows downward from JSIDX_OFFSET (128) toward 1.
    /// Panics if the borrow stack overflows (more than 127 borrowed refs in one operation).
    pub fn get_next_borrow_id(&mut self) -> u64 {
        if self.borrow_stack_pointer <= 1 {
            panic!("Borrow stack overflow: too many borrowed references in a single operation");
        }
        self.borrow_stack_pointer -= 1;
        self.borrow_stack_pointer
    }

    /// Push a borrow frame before a nested operation that may use borrowed refs.
    /// This saves the current borrow stack pointer so we can restore it later.
    pub fn push_borrow_frame(&mut self) {
        self.borrow_frame_stack.push(self.borrow_stack_pointer);
    }

    /// Pop a borrow frame after a nested operation completes.
    /// This restores the borrow stack pointer to where it was before the nested operation.
    pub fn pop_borrow_frame(&mut self) {
        if let Some(saved_pointer) = self.borrow_frame_stack.pop() {
            self.borrow_stack_pointer = saved_pointer;
        } else {
            panic!("pop_borrow_frame called with empty frame stack");
        }
    }

    /// Release a heap ID back to the free-list and queue it for JS drop.
    pub fn release_heap_id(&mut self, id: u64) -> Option<u64> {
        // Never release reserved IDs
        if id < JSIDX_RESERVED {
            unreachable!("Attempted to release reserved JS heap ID {}", id);
        }

        debug_assert!(
            !self.free_ids.contains(&id) && !self.ids_to_free.iter().any(|ids| ids.contains(&id)),
            "Double-free detected for heap ID {id}"
        );
        match self.ids_to_free.last_mut() {
            Some(ids) => {
                ids.push(id);
                None
            }
            None => {
                self.free_ids.push(id);
                Some(id)
            }
        }
    }

    /// Take the message data and reset the batch for reuse.
    /// Includes any pending drops at the start of the message.
    /// Prepends the reserved placeholder count so JS can skip those IDs during nested allocations.
    pub(crate) fn take_message(&mut self) -> IPCMessage {
        let reserved_count = self.take_reserved_placeholder_count();
        let mut encoder = self.take_encoder();
        encoder.prepend_u32(reserved_count);
        IPCMessage::new(encoder.to_bytes())
    }

    pub(crate) fn is_empty(&self) -> bool {
        // 12 bytes for offsets + 1 byte for message type
        self.encoder.byte_len() <= 13
    }

    pub(crate) fn push_ids_to_free(&mut self) {
        self.ids_to_free.push(Vec::new());
    }

    pub(crate) fn pop_and_release_ids(&mut self) -> Vec<u64> {
        let mut to_free = Vec::new();
        if let Some(ids) = self.ids_to_free.pop() {
            for id in ids {
                if let Some(freed_id) = self.release_heap_id(id) {
                    to_free.push(freed_id);
                }
            }
        }
        to_free
    }

    pub(crate) fn set_batching(&mut self, batching: bool) {
        self.is_batching = batching;
    }

    pub(crate) fn is_batching(&self) -> bool {
        self.is_batching
    }

    /// Take and reset the reserved placeholder count.
    /// Called when building a message to send to JS.
    pub(crate) fn take_reserved_placeholder_count(&mut self) -> u32 {
        core::mem::take(&mut self.reserved_placeholder_count)
    }

    pub(crate) fn take_encoder(&mut self) -> EncodedData {
        core::mem::replace(&mut self.encoder, Self::new_encoder_for_evaluate())
    }

    pub(crate) fn extend_encoder(&mut self, other: &EncodedData) {
        // Manually extend to avoid adding an extra message type byte
        self.encoder.u8_buf.extend_from_slice(&other.u8_buf[1..]);
        self.encoder.u16_buf.extend_from_slice(&other.u16_buf);
        self.encoder.u32_buf.extend_from_slice(&other.u32_buf);
        self.encoder.str_buf.extend_from_slice(&other.str_buf);
    }

    /// Get or create a type ID for the given type definition bytes.
    /// Returns (type_id, is_cached) where is_cached is true if the type was already in the cache.
    pub(crate) fn get_or_create_type_id(&mut self, type_bytes: Vec<u8>) -> (u32, bool) {
        if let Some(&id) = self.type_cache.get(&type_bytes) {
            (id, true)
        } else {
            let id = self.next_type_id;
            self.next_type_id += 1;
            self.type_cache.insert(type_bytes, id);
            (id, false)
        }
    }

    /// Insert an exported object and return its handle.
    pub(crate) fn insert_object<T: 'static>(&mut self, obj: T) -> u32 {
        let handle = self.next_object_handle;
        self.next_object_handle = self.next_object_handle.wrapping_add(1);
        self.objects.insert(handle, Box::new(RefCell::new(obj)));
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

    /// Get a reference to an exported object.
    pub(crate) fn get_object<T: 'static>(&self, handle: u32) -> Ref<'_, T> {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow()
    }

    /// Get a mutable reference to an exported object.
    pub(crate) fn get_object_mut<T: 'static>(&self, handle: u32) -> RefMut<'_, T> {
        let boxed = self.objects.get(&handle).expect("invalid handle");
        let cell = boxed.downcast_ref::<RefCell<T>>().expect("type mismatch");
        cell.borrow_mut()
    }

    /// Remove an exported object and return it.
    pub(crate) fn remove_object<T: 'static>(&mut self, handle: u32) -> T {
        let boxed = self.objects.remove(&handle).expect("invalid handle");
        let cell = boxed.downcast::<RefCell<T>>().expect("type mismatch");
        cell.into_inner()
    }

    /// Remove an exported object without returning it.
    pub(crate) fn remove_object_untyped(&mut self, handle: u32) -> Option<Box<dyn Any>> {
        self.objects.remove(&handle)
    }

    /// Get a reference to the IPC layer.
    pub(crate) fn ipc(&self) -> &WryIPC {
        &self.ipc
    }

    /// Get the webview ID associated with this runtime.
    pub(crate) fn webview_id(&self) -> u64 {
        self.webview_id
    }
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
pub fn is_batching() -> bool {
    with_runtime(|state| state.is_batching())
}

/// Queue a JS drop operation for a heap ID.
/// This is called when a JsValue is dropped.
pub(crate) fn queue_js_drop(id: u64) {
    debug_assert!(
        id >= JSIDX_RESERVED,
        "Attempted to drop reserved JS heap ID {id}"
    );

    let runtime_already_dropped = RUNTIME.with(|state| state.borrow().is_empty());
    // If the runtime has already been dropped, we don't need to drop the JS reference
    if runtime_already_dropped {
        return;
    }

    let id = with_runtime(|state| state.release_heap_id(id));
    if let Some(id) = id {
        crate::js_helpers::js_drop_heap_ref(id);
    }
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
        state.push_ids_to_free();
        state.take_encoder()
    });
    add_operation(&mut batch, fn_id, add_args);

    // Check if any encoded argument requires immediate flush (e.g., stack-allocated callbacks)
    let needs_flush = batch.needs_flush;

    with_runtime(|state| {
        let encoded_during_op = core::mem::replace(&mut state.encoder, batch);
        state.extend_encoder(&encoded_during_op);
    });

    // Try to get a placeholder for opaque types that don't need flush
    // This also increments opaque_count to keep heap IDs in sync
    let get_placeholder = || with_runtime(|state| R::try_placeholder(state));

    // Must flush if: not batching, or if the operation requires immediate execution
    // (e.g., stack-allocated callbacks that must be invoked before returning)
    let result = if !is_batching() || needs_flush {
        flush_and_then(|mut data| {
            let response = get_placeholder()
                .unwrap_or_else(|| R::decode(&mut data).expect("Failed to decode return value"));
            assert!(
                data.is_empty(),
                "Extra data remaining after decoding response"
            );
            response
        })
    } else {
        get_placeholder().unwrap_or_else(|| flush_and_return::<R>())
    };

    // After running, free any queued IDs for this operation
    let ids = with_runtime(|state| state.pop_and_release_ids());
    for id in ids {
        crate::js_helpers::js_drop_heap_ref(id);
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

pub(crate) fn flush_and_then<R>(then: impl for<'a> Fn(DecodedData<'a>) -> R) -> R {
    use crate::runtime::WryBindgenEvent;

    let batch_msg = with_runtime(|state| state.take_message());

    // Send and wait for result
    with_runtime(|runtime| {
        (runtime.ipc().proxy)(WryBindgenEvent::ipc(runtime.webview_id(), batch_msg))
    });
    loop {
        if let Some(result) = crate::runtime::progress_js_with(&then) {
            return result;
        }
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

pub fn force_flush() {
    let has_pending = with_runtime(|state| !state.is_empty());
    if has_pending {
        flush_and_return::<()>();
    }
}
