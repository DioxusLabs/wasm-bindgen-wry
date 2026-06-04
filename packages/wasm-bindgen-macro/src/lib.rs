//! Shim macro that expands to both wasm-bindgen and wry-bindgen implementations
//!
//! This macro generates cfg-conditional code so the same `#[wasm_bindgen]` code
//! compiles correctly for both wasm32 and desktop targets.
//!
//! For wasm32 targets: Re-emits the input with the upstream wasm_bindgen attribute
//! For non-wasm32 targets: Expands using wry-bindgen-macro-support

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

/// The main wasm_bindgen attribute macro.
///
/// This expands to both wasm-bindgen (for wasm32) and wry-bindgen (for desktop)
/// using cfg attributes directly on the generated code.
#[proc_macro_attribute]
pub fn wasm_bindgen(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let input2: TokenStream2 = input.into();

    // Expand wry-bindgen for non-wasm32 targets
    let wry_expansion = match wry_bindgen_macro_support::expand(attr2.clone(), input2.clone()) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    };

    // For wasm32: Re-emit with the upstream wasm_bindgen attribute
    // For non-wasm32: Use the wry-bindgen expansion directly
    //
    // We emit both expansions with cfg guards. The wry_expansion contains all the
    // generated items (functions, types, impls) which will be at the same scope level
    // as the original input would have been.
    let output = quote! {
        #[cfg(target_arch = "wasm32")]
        #[::wasm_bindgen::__wasm_bindgen_upstream_macro(#attr2)]
        #input2

        #[cfg(not(target_arch = "wasm32"))]
        #wry_expansion
    };

    output.into()
}

/// The link_to proc-macro for JavaScript module linking.
///
/// This only works on wasm32 targets; on desktop it panics.
#[proc_macro]
pub fn link_to(input: TokenStream) -> TokenStream {
    let input2: TokenStream2 = input.into();

    // link_to only makes sense on wasm32 - delegate to upstream macro there
    let output = quote! {
        {
            #[cfg(target_arch = "wasm32")]
            {
                ::wasm_bindgen::__wasm_bindgen_upstream_link_to!(#input2)
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                panic!("link_to! cannot be used outside of wasm32 target")
            }
        }
    };

    output.into()
}

/// Internal class marker macro for impl method expansion.
///
/// This is used internally by both wasm-bindgen and wry-bindgen for handling
/// methods within impl blocks.
#[proc_macro_attribute]
pub fn __wasm_bindgen_class_marker(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr2: TokenStream2 = attr.into();
    let input2: TokenStream2 = input.into();

    // For wry-bindgen, we need to expand the regular macro on the method
    let wry_expansion = match wry_bindgen_macro_support::expand(attr2.clone(), input2.clone()) {
        Ok(tokens) => tokens,
        Err(e) => e.to_compile_error(),
    };

    // For wasm32: delegate to upstream class marker
    let output = quote! {
        #[cfg(target_arch = "wasm32")]
        #[::wasm_bindgen::__wasm_bindgen_upstream_class_marker(#attr2)]
        #input2

        #[cfg(not(target_arch = "wasm32"))]
        #wry_expansion
    };

    output.into()
}
