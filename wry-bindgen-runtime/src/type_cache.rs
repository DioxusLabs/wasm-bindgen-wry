//! Cache for Rust function type definitions sent to JavaScript.
//!
//! Type IDs assigned by [`get_or_create_type_id`](TypeCache::get_or_create_type_id)
//! are not safe to send as `TYPE_CACHED` until JS has actually parsed the
//! matching `TYPE_FULL`. Under the strict synchronous ping-pong IPC, JS parses
//! a Rust-to-JS Evaluate before sending its Respond, so the ack lifecycle is
//! a plain stack: each outbound Evaluate pushes its newly-introduced type
//! IDs onto `awaiting_ack_stack`, and the matching JS Respond pops and acks
//! them. No request IDs needed.

use crate::wire::TypeDef;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

/// Assigns stable IDs to type definitions for a runtime.
pub(crate) struct TypeCache {
    /// Type definitions mapped to assigned IDs.
    ids: BTreeMap<TypeDef, u32>,
    /// IDs JS has acked, so future uses can send `TYPE_CACHED`. Keyed by ID
    /// directly because both acking and the cached-check are ID-based.
    acked: BTreeSet<u32>,
    /// Each frame is the set of type IDs introduced by one outbound Rust→JS
    /// Evaluate, in send order. The matching JS Respond pops the top frame.
    awaiting_ack_stack: Vec<Vec<u32>>,
    next_type_id: u32,
}

impl TypeCache {
    pub(crate) fn new() -> Self {
        Self {
            ids: BTreeMap::new(),
            acked: BTreeSet::new(),
            awaiting_ack_stack: Vec::new(),
            next_type_id: 0,
        }
    }

    /// Get or create a type ID for the given type definition.
    ///
    /// Returns `(type_id, can_use_cached)`, where `can_use_cached` is true only
    /// after JS has acknowledged parsing a full definition for the ID.
    pub(crate) fn get_or_create_type_id(&mut self, type_def: &TypeDef) -> (u32, bool) {
        if let Some(&id) = self.ids.get(type_def) {
            (id, self.acked.contains(&id))
        } else {
            let id = self.next_type_id;
            self.next_type_id += 1;
            self.ids.insert(type_def.clone(), id);
            (id, false)
        }
    }

    /// Push a frame of type IDs introduced by an outbound Evaluate. The next
    /// inbound JS Respond pops and acks them.
    pub(crate) fn push_pending_frame(&mut self, type_ids: Vec<u32>) {
        self.awaiting_ack_stack.push(type_ids);
    }

    /// Pop the top pending frame and mark its types as acked.
    pub(crate) fn pop_and_ack_pending_frame(&mut self) {
        if let Some(type_ids) = self.awaiting_ack_stack.pop() {
            self.ack_type_ids(&type_ids);
        }
    }

    /// Mark the given type IDs as acked by JS so future uses can send
    /// `TYPE_CACHED`. Used for types introduced in an outbound Respond, which JS
    /// processes synchronously before Rust runs again (so no frame is needed).
    pub(crate) fn ack_type_ids(&mut self, type_ids: &[u32]) {
        self.acked.extend(type_ids.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_id_is_cached_after_pending_frame_is_acked() {
        let mut cache = TypeCache::new();
        let type_def = TypeDef::of::<u8>();

        let (id, cached) = cache.get_or_create_type_id(&type_def);
        assert_eq!(id, 0);
        assert!(!cached);

        cache.push_pending_frame(vec![id]);
        assert_eq!(cache.get_or_create_type_id(&type_def), (id, false));

        cache.pop_and_ack_pending_frame();
        assert_eq!(cache.get_or_create_type_id(&type_def), (id, true));
    }

    #[test]
    fn ack_only_caches_types_in_the_top_frame() {
        let mut cache = TypeCache::new();
        let first = TypeDef::of::<u8>();
        let second = TypeDef::of::<u16>();

        let (first_id, _) = cache.get_or_create_type_id(&first);
        let (second_id, _) = cache.get_or_create_type_id(&second);

        // Two outbound Evaluates in flight; the inner one (second_id) gets
        // acked first via stack order.
        cache.push_pending_frame(vec![first_id]);
        cache.push_pending_frame(vec![second_id]);

        cache.pop_and_ack_pending_frame();

        assert_eq!(cache.get_or_create_type_id(&first), (first_id, false));
        assert_eq!(cache.get_or_create_type_id(&second), (second_id, true));
    }

    #[test]
    fn empty_pop_is_a_noop() {
        let mut cache = TypeCache::new();
        cache.pop_and_ack_pending_frame();
        let type_def = TypeDef::of::<u8>();
        let (id, cached) = cache.get_or_create_type_id(&type_def);
        assert_eq!(id, 0);
        assert!(!cached);
    }
}
