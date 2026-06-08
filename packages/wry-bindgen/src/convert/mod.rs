//! Conversion traits for wasm-bindgen API compatibility.
//!
//! These traits provide compatibility with code that uses wasm-bindgen's
//! low-level ABI conversion types. Wry-bindgen keeps the upstream module shape,
//! but the actual transport is Wry's binary protocol rather than raw Wasm ABI
//! primitives.

mod arg;
mod closures;
mod impls;
mod slices;
mod traits;

pub use self::arg::*;
pub use self::impls::*;
pub use self::slices::*;
pub use self::traits::*;
