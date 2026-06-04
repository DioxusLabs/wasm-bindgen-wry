use wasm_bindgen::prelude::*;
use wasm_bindgen::wasm_bindgen;

/// Reproduction target for opaque heap id aliasing under batched, interleaved Evaluates.
///
/// The important shape is:
/// - keep several batched HeapRef placeholders live,
/// - force explicit HeapRef allocations through Result<JsValue, JsValue>,
/// - repeat that shape many times inside one outer batch,
/// - drop all explicit returns and placeholders after each burst.
///
/// If JS allocates an explicit HeapRef id that Rust has already handed out for a
/// placeholder, Rust ends up with two `JsValue`s owning the same heap id. The
/// protocol must prevent that alias from being constructed in the first place.
pub(crate) async fn test_opaque_id_double_free_stress() {
    #[wasm_bindgen(inline_js = r#"
        export function make_heap_ref(seed, slot) {
            return { kind: "placeholder", seed, slot, createdAt: performance.now() };
        }

        export function explicit_heap_ref(value, seed, slot) {
            return { kind: "explicit", value, seed, slot, createdAt: performance.now() };
        }
    "#)]
    extern "C" {
        fn make_heap_ref(seed: u32, slot: u32) -> JsValue;

        #[wasm_bindgen(catch)]
        fn explicit_heap_ref(value: &JsValue, seed: u32, slot: u32) -> Result<JsValue, JsValue>;
    }

    fn exercise(seed: u32) {
        const PLACEHOLDERS_PER_FLUSH: u32 = 6;
        const EXPLICIT_PER_PLACEHOLDER: u32 = 2;

        let mut placeholders = Vec::with_capacity(PLACEHOLDERS_PER_FLUSH as usize);
        for slot in 0..PLACEHOLDERS_PER_FLUSH {
            placeholders.push(make_heap_ref(seed, slot));
        }

        let mut explicit_returns =
            Vec::with_capacity((PLACEHOLDERS_PER_FLUSH * EXPLICIT_PER_PLACEHOLDER) as usize);
        for (placeholder_slot, placeholder) in placeholders.iter().enumerate() {
            for explicit_slot in 0..EXPLICIT_PER_PLACEHOLDER {
                explicit_returns.push(
                    explicit_heap_ref(
                        placeholder,
                        seed,
                        placeholder_slot as u32 * EXPLICIT_PER_PLACEHOLDER + explicit_slot,
                    )
                    .unwrap(),
                );
            }
        }

        drop(explicit_returns);
        drop(placeholders);
    }

    let started = std::time::Instant::now();
    println!("[opaque_id_stress] t=0ms start");

    for seed in 0..32 {
        exercise(seed);
    }

    println!(
        "[opaque_id_stress] completed without opaque-id aliasing in {}ms",
        started.elapsed().as_millis()
    );
}
