//! Hooks for responding to hard-abort and reinit events on the Wasm instance.

#[doc(hidden)]
pub use crate::__rt::schedule_reinit;
#[doc(hidden)]
pub use crate::__rt::set_on_abort;
