//! Arity-generic exported-function argument metadata.
//!
//! Export invocation is generated at each wrapper site because projection is
//! lifetime-parameterized: a borrowed argument like `&str` projects through
//! `ArgAbiProject<'a, S>` as `&'a str`, and the concrete borrow lifetime is only
//! known where the decoded anchor is projected. The tuple machinery here stays
//! lifetime-free because it only asks each argument for wire type metadata.

use crate::__rt::alloc::vec::Vec;
use crate::encode::{ArgAbi, BorrowScope, TypeDef};

/// Argument type metadata for an exported function's `Args` tuple in borrow
/// scope `S`.
#[doc(hidden)]
pub trait CallExportArgs<S: BorrowScope> {
    fn arg_types() -> Vec<TypeDef>;
}

/// Implement [`CallExportArgs`] for one arity.
macro_rules! impl_call_export_args {
    ( $( $A:ident )* ) => {
        impl<S, $($A,)*> CallExportArgs<S> for ($($A,)*)
        where
            S: BorrowScope,
            $( $A: ArgAbi<S>, )*
        {
            #[allow(non_snake_case)]
            fn arg_types() -> Vec<TypeDef> {
                #[allow(unused_mut)]
                let mut types = Vec::new();
                $(
                    types.push(TypeDef::of::<<$A as ArgAbi<S>>::Wire>());
                )*
                types
            }
        }
    };
}

// Exports may take any number of value arguments; cap at 16 (double the closure
// upcast macro's 8). Extend the list to raise the metadata arity cap.
impl_call_export_args!();
impl_call_export_args!(A0);
impl_call_export_args!(A0 A1);
impl_call_export_args!(A0 A1 A2);
impl_call_export_args!(A0 A1 A2 A3);
impl_call_export_args!(A0 A1 A2 A3 A4);
impl_call_export_args!(A0 A1 A2 A3 A4 A5);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14);
impl_call_export_args!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15);
