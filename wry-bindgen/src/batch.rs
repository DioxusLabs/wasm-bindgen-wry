//! Batching system for grouping multiple JS operations into single messages.
//!
//! This module provides the batching infrastructure that allows multiple
//! JS operations to be grouped together for efficient execution.

use std::cell::RefCell;

use crate::encode::{BatchableResult, BinaryDecode};
use crate::function::JSFunction;
use crate::ipc::{EncodedData, IPCMessage, MessageType};
use crate::runtime::get_runtime;
use crate::value::{DROP_HEAP_REF_FN_ID, JSIDX_RESERVED};

/// State for batching operations.
/// Every evaluation is a batch - it may just have one operation.
///
/// Uses a free-list strategy for heap ID allocation to stay in sync with JS heap.
pub struct BatchState {
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
}

impl BatchState {
    pub(crate) fn new() -> Self {
        Self {
            encoder: Self::new_encoder_for_evaluate(),
            free_ids: Vec::new(),
            // Start allocating heap IDs from JSIDX_RESERVED to match JS heap
            max_id: JSIDX_RESERVED,
            ids_to_free: Vec::new(),
            is_batching: false,
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
        if let Some(id) = self.free_ids.pop() {
            id
        } else {
            let id = self.max_id;
            self.max_id += 1;
            id
        }
    }

    /// Release a heap ID back to the free-list and queue it for JS drop.
    pub fn release_heap_id(&mut self, id: u64) {
        // Never release reserved IDs
        if id < JSIDX_RESERVED {
            unreachable!("Attempted to release reserved JS heap ID {}", id);
        }

        debug_assert!(
            !self.free_ids.contains(&id) && !self.ids_to_free.iter().any(|ids| ids.contains(&id)),
            "Double-free detected for heap ID {}",
            id
        );
        match self.ids_to_free.last_mut() {
            Some(ids) => ids.push(id),
            None => self.free_ids.push(id),
        }
    }

    /// Take the message data and reset the batch for reuse.
    /// Includes any pending drops at the start of the message.
    pub(crate) fn take_message(&mut self) -> IPCMessage {
        IPCMessage::new(self.take_encoder().to_bytes())
    }

    pub(crate) fn is_empty(&self) -> bool {
        // 12 bytes for offsets + 1 byte for message type
        self.encoder.byte_len() <= 13
    }

    pub(crate) fn push_ids_to_free(&mut self) {
        self.ids_to_free.push(Vec::new());
    }

    pub(crate) fn pop_and_release_ids(&mut self) {
        if let Some(ids) = self.ids_to_free.pop() {
            for id in ids {
                self.release_heap_id(id);
            }
        }
    }

    pub(crate) fn set_batching(&mut self, batching: bool) {
        self.is_batching = batching;
    }

    pub(crate) fn is_batching(&self) -> bool {
        self.is_batching
    }

    pub(crate) fn take_encoder(&mut self) -> EncodedData {
        std::mem::replace(&mut self.encoder, Self::new_encoder_for_evaluate())
    }

    pub(crate) fn extend_encoder(&mut self, other: &EncodedData) {
        // Manually extend to avoid adding an extra message type byte
        self.encoder.u8_buf.extend_from_slice(&other.u8_buf[1..]);
        self.encoder.u16_buf.extend_from_slice(&other.u16_buf);
        self.encoder.u32_buf.extend_from_slice(&other.u32_buf);
        self.encoder.str_buf.extend_from_slice(&other.str_buf);
    }
}

thread_local! {
    /// Thread-local batch state - always exists, reset after each flush
    pub(crate) static BATCH_STATE: RefCell<BatchState> = RefCell::new(BatchState::new());
}

/// Check if we're currently inside a batch() call
pub fn is_batching() -> bool {
    BATCH_STATE.with(|state| state.borrow().is_batching())
}

/// Queue a JS drop operation for a heap ID.
/// This is called when a JsValue is dropped.
pub(crate) fn queue_js_drop(id: u64) {
    debug_assert!(
        id >= JSIDX_RESERVED,
        "Attempted to drop reserved JS heap ID {}",
        id
    );
    let drop_fn: JSFunction<fn(u64) -> ()> = JSFunction::new(DROP_HEAP_REF_FN_ID);
    drop_fn.call(id);
    BATCH_STATE.with(|state| {
        state.borrow_mut().release_heap_id(id);
    });
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
    // Always batch together the call and any potential drops that may happen during encoding
    batch(|| {
        // Step 1: Encode the operation into the batch and get placeholder for non-flush types
        // We take the current encoder out of the thread-local state to avoid borrowing issues
        // and then put it back after adding the operation. Drops or other calls may happen while
        // we are encoding, but they should be queued after this operation.
        let mut batch = BATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            // Push a new operation into the batch
            state.push_ids_to_free();
            state.take_encoder()
        });
        add_operation(&mut batch, fn_id, add_args);

        BATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let encoded_during_op = std::mem::replace(&mut state.encoder, batch);
            state.extend_encoder(&encoded_during_op);
        });

        // Get placeholder for types that don't need flush
        // This also increments opaque_count to keep heap IDs in sync
        let result = if !R::needs_flush() {
            let placeholder = BATCH_STATE.with(|state| {
                let mut state = state.borrow_mut();
                R::batched_placeholder(&mut state)
            });
            if !is_batching() {
                flush_and_return::<R>()
            } else {
                placeholder
            }
        } else {
            flush_and_return::<R>()
        };

        // After running, free any queued IDs for this operation
        BATCH_STATE.with(|state| {
            state.borrow_mut().pop_and_release_ids();
        });

        result
    })
}

/// Flush the current batch and return the decoded result.
pub(crate) fn flush_and_return<R: BinaryDecode>() -> R {
    use crate::runtime::AppEvent;

    let batch_msg = BATCH_STATE.with(|state| state.borrow_mut().take_message());

    // Send and wait for result
    let runtime = get_runtime();
    (runtime.proxy)(AppEvent::Ipc(batch_msg));
    let result: R = crate::runtime::wait_for_js_result();

    result
}

/// Execute operations inside a batch. Operations that return opaque types (like JsValue)
/// will be batched and executed together. Operations that return non-opaque types will
/// flush the batch to get the actual result.
pub fn batch<R, F: FnOnce() -> R>(f: F) -> R {
    let currently_batching = is_batching();
    // Start batching
    BATCH_STATE.with(|state| state.borrow_mut().set_batching(true));

    // Execute the closure
    let result = f();

    if !currently_batching {
        // Flush any remaining batched operations
        let has_pending = BATCH_STATE.with(|state| !state.borrow().is_empty());
        if has_pending {
            flush_and_return::<()>();
        }
    }

    // End batching
    BATCH_STATE.with(|state| state.borrow_mut().set_batching(currently_batching));

    result
}
