//! ID allocation for JavaScript references and Rust-owned object handles.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::value::{JSIDX_OFFSET, JSIDX_RESERVED};

/// An integer usable as a slab ID. Implemented for the widths the slab is
/// instantiated at (`u64` for heap IDs, `u32` for object handles).
pub(crate) trait SlabId: Ord + Copy + core::fmt::Display {
    /// The next ID after `self`, or `None` if the ID space is exhausted.
    fn next(self) -> Option<Self>;
}

impl SlabId for u32 {
    fn next(self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl SlabId for u64 {
    fn next(self) -> Option<Self> {
        self.checked_add(1)
    }
}

/// A small ID-only slab.
///
/// IDs move through three states:
/// - unallocated,
/// - live, after `alloc` or `reserve_exact`,
/// - released, after `release` but before `recycle`.
///
/// Keeping `release` and `recycle` separate lets callers delay reuse until the
/// remote side has completed any required cleanup.
pub(crate) struct IdSlab<T: SlabId> {
    live_ids: BTreeSet<T>,
    free_ids: BTreeSet<T>,
    next_id: T,
}

impl<T: SlabId> IdSlab<T> {
    pub(crate) fn new(first_id: T) -> Self {
        Self {
            live_ids: BTreeSet::new(),
            free_ids: BTreeSet::new(),
            next_id: first_id,
        }
    }

    pub(crate) fn alloc(&mut self) -> T {
        if let Some(id) = self.free_ids.iter().next().copied() {
            self.free_ids.remove(&id);
            self.mark_live(id);
            return id;
        }

        let id = self.next_id;
        self.next_id = self.next_id.next().expect("ID space exhausted");
        self.mark_live(id);
        id
    }

    fn release(&mut self, id: T) {
        assert!(
            self.live_ids.remove(&id),
            "Attempted to release ID {id}, but it is not live"
        );
    }

    fn recycle(&mut self, id: T) {
        assert!(
            !self.live_ids.contains(&id),
            "Attempted to recycle ID {id}, but it is still live"
        );

        self.free_ids.insert(id);
    }

    fn recycle_if_released(&mut self, id: T) -> bool {
        if self.live_ids.contains(&id) {
            return false;
        }

        self.free_ids.insert(id);
        true
    }

    /// Release an ID without putting it back on the free list.
    ///
    /// Use this for IDs that have escaped to JavaScript without a generation
    /// token. JS finalizers can run later than Rust-side drop, so reusing the
    /// raw ID would let a stale finalizer affect a different object.
    pub(crate) fn retire(&mut self, id: T) {
        self.release(id);
    }

    /// Whether `id` is currently allocated (live).
    pub(crate) fn contains(&self, id: T) -> bool {
        self.live_ids.contains(&id)
    }

    fn mark_live(&mut self, id: T) {
        assert!(self.live_ids.insert(id), "ID {id} is already live");
    }
}

/// Heap IDs Rust allocated for values JS sent without encoding an ID. Sent
/// back to JS so it can install them into its heap at those slots.
pub(crate) type InstallIdBatch = Vec<u64>;

pub(crate) struct HeapIds {
    slab: IdSlab<u64>,
    /// IDs allocated while decoding the current inbound message, awaiting the
    /// next outbound's install prelude. Every outbound drains this via
    /// `take_pending_install_ids`, and the protocol decodes exactly one inbound
    /// between consecutive outbounds, so at each drain this holds exactly that
    /// one message's IDs — no per-message framing is needed.
    pending_install_ids: Vec<u64>,
    /// IDs reserved as placeholders for JS function return values.
    reserved_placeholder_ids: Vec<u64>,
}

impl HeapIds {
    pub(crate) fn new() -> Self {
        Self {
            slab: IdSlab::new(JSIDX_RESERVED),
            pending_install_ids: Vec::new(),
            reserved_placeholder_ids: Vec::new(),
        }
    }

    fn next_heap_id(&mut self) -> u64 {
        self.slab.alloc()
    }

    /// Get the next heap ID for a return value placeholder.
    pub(crate) fn next_placeholder_id(&mut self) -> u64 {
        let id = self.next_heap_id();
        self.reserved_placeholder_ids.push(id);
        id
    }

    pub(crate) fn next_inbound_js_heap_id(&mut self) -> u64 {
        let id = self.slab.alloc();
        self.pending_install_ids.push(id);
        id
    }

    /// Release a heap ID's slab slot. Whether JS should be notified now is
    /// decided by the runtime's operation-free batching, not here.
    pub(crate) fn release_heap_slot(&mut self, id: u64) {
        if id < JSIDX_RESERVED {
            unreachable!("Attempted to release reserved JS heap ID {}", id);
        }

        self.slab.release(id);
    }

    pub(crate) fn recycle_heap_id(&mut self, id: u64) {
        if id >= JSIDX_RESERVED {
            self.slab.recycle(id);
        }
    }

    pub(crate) fn recycle_heap_id_if_released(&mut self, id: u64) -> bool {
        id >= JSIDX_RESERVED && self.slab.recycle_if_released(id)
    }

    /// Take the IDs JS should install for objects it sent to Rust. Empty when
    /// the last inbound message carried no heap refs.
    pub(crate) fn take_pending_install_ids(&mut self) -> InstallIdBatch {
        core::mem::take(&mut self.pending_install_ids)
    }

    /// Take IDs JS should reserve for pending Rust-to-JS return values.
    pub(crate) fn take_reserved_placeholder_ids(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.reserved_placeholder_ids)
    }
}

pub(crate) struct BorrowIds {
    /// Borrow stack pointer - uses indices 1-127, growing downward from
    /// JSIDX_OFFSET (128) to 1. Reset after each operation completes.
    stack_pointer: u64,
    /// Frame stack for nested operations - saves borrow stack pointers.
    frame_stack: Vec<u64>,
}

impl BorrowIds {
    pub(crate) fn new() -> Self {
        Self {
            stack_pointer: JSIDX_OFFSET,
            frame_stack: Vec::new(),
        }
    }

    /// Get the next borrow ID from the borrow stack (indices 1-127).
    ///
    /// The borrow stack grows downward from JSIDX_OFFSET (128) toward 1.
    /// Panics if the borrow stack overflows.
    pub(crate) fn next_borrow_id(&mut self) -> u64 {
        if self.stack_pointer <= 1 {
            panic!("Borrow stack overflow: too many borrowed references in a single operation");
        }
        self.stack_pointer -= 1;
        self.stack_pointer
    }

    /// Push a borrow frame before a nested operation that may use borrowed refs.
    pub(crate) fn push_frame(&mut self) {
        self.frame_stack.push(self.stack_pointer);
    }

    /// Pop a borrow frame after a nested operation completes.
    pub(crate) fn pop_frame(&mut self) {
        if let Some(saved_pointer) = self.frame_stack.pop() {
            self.stack_pointer = saved_pointer;
        } else {
            panic!("pop_borrow_frame called with empty frame stack");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_reuses_recycled_ids_in_order() {
        let mut slab = IdSlab::new(10_u64);
        let first = slab.alloc();
        let second = slab.alloc();
        assert_eq!(first, 10);
        assert_eq!(second, 11);

        slab.release(second);
        slab.recycle(second);
        slab.release(first);
        slab.recycle(first);

        assert_eq!(slab.alloc(), 10);
        assert_eq!(slab.alloc(), 11);
    }

    #[test]
    fn heap_ids_start_at_js_reserved_and_reuse_after_recycle() {
        let mut heap = HeapIds::new();
        let id = heap.next_placeholder_id();
        assert_eq!(id, JSIDX_RESERVED);
        heap.release_heap_slot(id);
        heap.recycle_heap_id(id);
        assert_eq!(heap.next_placeholder_id(), id);
    }

    #[test]
    fn inbound_drop_uses_normal_release_path() {
        let mut heap = HeapIds::new();
        let id = heap.next_inbound_js_heap_id();
        heap.release_heap_slot(id);

        let next = heap.next_placeholder_id();
        assert_ne!(next, id);

        let batch = heap.take_pending_install_ids();
        assert_eq!(batch, vec![id]);

        heap.recycle_heap_id(id);
        assert_eq!(heap.next_placeholder_id(), id);
    }

    #[test]
    fn inbound_heap_ids_collapse_into_one_install_batch() {
        let mut heap = HeapIds::new();
        let ids = [
            heap.next_inbound_js_heap_id(),
            heap.next_inbound_js_heap_id(),
        ];
        assert_eq!(ids, [JSIDX_RESERVED, JSIDX_RESERVED + 1]);

        let batch = heap.take_pending_install_ids();
        assert_eq!(batch, ids);
        assert!(heap.take_pending_install_ids().is_empty());
    }

    #[test]
    fn taking_with_no_inbound_ids_is_empty() {
        let mut heap = HeapIds::new();
        assert!(heap.take_pending_install_ids().is_empty());
    }

    #[test]
    fn each_take_drains_only_ids_since_the_last_take() {
        let mut heap = HeapIds::new();
        let first = heap.next_inbound_js_heap_id();
        assert_eq!(heap.take_pending_install_ids(), vec![first]);

        let second = heap.next_inbound_js_heap_id();
        assert_eq!(heap.take_pending_install_ids(), vec![second]);
    }

    #[test]
    fn borrow_frames_restore_stack_pointer() {
        let mut borrows = BorrowIds::new();
        assert_eq!(borrows.next_borrow_id(), JSIDX_OFFSET - 1);
        borrows.push_frame();
        assert_eq!(borrows.next_borrow_id(), JSIDX_OFFSET - 2);
        assert_eq!(borrows.next_borrow_id(), JSIDX_OFFSET - 3);
        borrows.pop_frame();
        assert_eq!(borrows.next_borrow_id(), JSIDX_OFFSET - 2);
    }

    #[test]
    #[should_panic(expected = "Borrow stack overflow")]
    fn borrow_stack_panics_on_overflow() {
        let mut borrows = BorrowIds::new();
        for _ in 0..JSIDX_OFFSET {
            borrows.next_borrow_id();
        }
    }

    #[test]
    fn object_handles_can_retire_released_handles_without_reuse() {
        let mut handles = IdSlab::new(1_u32);
        let first = handles.alloc();
        let second = handles.alloc();
        assert_eq!(first, 1);
        handles.retire(first);
        assert_ne!(handles.alloc(), first);
        assert_eq!(second, first + 1);
    }

    #[test]
    #[should_panic(expected = "ID space exhausted")]
    fn object_handles_panic_on_overflow() {
        let mut handles = IdSlab::new(u32::MAX);
        handles.alloc();
    }
}
