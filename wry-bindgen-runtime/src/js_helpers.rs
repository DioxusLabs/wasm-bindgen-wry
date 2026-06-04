use alloc::string::String;

use crate::batch::{run_js_sync, with_runtime};
use wry_bindgen_abi::{BinaryEncode, FunctionTypeInfo, JsFunctionSpec, TypeDef};

fn drop_heap_ref_js() -> String {
    "heapId => window.jsHeap.remove(heapId)".into()
}

static DROP_HEAP_REF: JsFunctionSpec = JsFunctionSpec::new(drop_heap_ref_js);

inventory::submit! {
    DROP_HEAP_REF
}

fn dispose_rust_function_js() -> String {
    r#"heapId => {
  const value = window.jsHeap.get(heapId);
  const rustFunction = value && value.__wryRustFunction;
  if (rustFunction && typeof rustFunction.disposeFromRust === "function") {
    rustFunction.disposeFromRust();
  }
}"#
    .into()
}

static DISPOSE_RUST_FUNCTION: JsFunctionSpec = JsFunctionSpec::new(dispose_rust_function_js);

inventory::submit! {
    DISPOSE_RUST_FUNCTION
}

pub(crate) fn js_drop_heap_ref(heap_id: u64) {
    call_heap_helper(DROP_HEAP_REF, heap_id);
}

pub(crate) fn js_dispose_rust_function(heap_id: u64) {
    call_heap_helper(DISPOSE_RUST_FUNCTION, heap_id);
}

fn call_heap_helper(spec: JsFunctionSpec, heap_id: u64) {
    let fn_id = with_runtime(|runtime| runtime.resolve_function(spec));
    run_js_sync::<()>(
        fn_id,
        move |encoder| {
            let type_def = TypeDef::of::<fn(u64) -> ()>();
            let (type_id, can_use_cached) =
                with_runtime(|runtime| runtime.get_or_create_type_id(&type_def));
            FunctionTypeInfo::new(type_id, can_use_cached, &type_def).encode(encoder);
            heap_id.encode(encoder);
            false
        },
        |_| Some(()),
        |_| (),
    );
}
