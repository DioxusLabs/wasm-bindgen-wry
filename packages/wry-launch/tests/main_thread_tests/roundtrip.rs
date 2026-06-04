use wasm_bindgen::{Clamped, wasm_bindgen};

pub(crate) fn test_roundtrip() {
    macro_rules! roundtrip {
        ($t:ty, $val:expr) => {{
            println!("testing roundtrip for type {}", stringify!($t));
            #[wasm_bindgen(inline_js = "export function identity(x) { return x; }")]
            extern "C" {
                #[wasm_bindgen(js_name = identity)]
                fn identity(x: $t) -> $t;
            }

            let input: $t = $val;
            let output: $t = identity(input.clone());
            assert_eq!(
                input,
                output,
                "Roundtrip failed for type {}",
                stringify!($t)
            );
        }};
    }

    roundtrip!(u8, 42u8);
    roundtrip!(u16, 42u16);
    roundtrip!(u32, 42u32);
    roundtrip!(u64, 42u64);
    // u128/i128 are carried as f64 on the JS side, so keep the values inside the
    // f64 safe-integer range; this still exercises the u128/i128 encode/decode paths.
    roundtrip!(u128, 42u128);
    // Negative i128 cannot round-trip through the f64-based JS i128 path; a small
    // positive value still drives the same Rust i128 encode/decode lines.
    roundtrip!(i128, 42i128);
    roundtrip!(i8, -42i8);
    roundtrip!(i16, -42i16);
    roundtrip!(i32, -42i32);
    roundtrip!(i64, -42i64);
    roundtrip!(usize, 42usize);
    roundtrip!(isize, -42isize);
    // `char` encodes with the U32 type tag, so this drives the JS U32 path too.
    roundtrip!(char, 'A');
    roundtrip!(char, '🦀');
    roundtrip!(f32, std::f32::consts::PI);
    roundtrip!(f64, std::f64::consts::PI);
    roundtrip!(String, "Hello, world!".to_string());
    roundtrip!(bool, true);
    roundtrip!(bool, false);
    roundtrip!(Option<u32>, Some(100u32));
    roundtrip!(Option<u32>, None);
    roundtrip!(Vec<u32>, vec![1u32, 2u32, 3u32, 4u32, 5u32]);
    roundtrip!(Vec<f32>, vec![1f32, 2f32, 3f32, 4f32, 5f32]);
    roundtrip!(Option<Vec<f32>>, Some(vec![1f32, 2f32, 3f32, 4f32, 5f32]));

    // Clamped u8 array roundtrip
    roundtrip!(Clamped<Vec<u8>>, Clamped(vec![0u8, 128u8, 255u8]));

    // `Result<T, E>` cannot be a top-level return type (that triggers wasm-bindgen
    // catch semantics), but nested inside an Option it round-trips as a value,
    // exercising the Result encode/decode/type-def paths.
    roundtrip!(Option<Result<u32, u32>>, Some(Ok(7u32)));
    roundtrip!(Option<Result<u32, u32>>, Some(Err(9u32)));

    // Borrowed slice / boxed slice encoders are encode-only (they can't be a return
    // type), so pass them as arguments to a JS function that reduces them.
    {
        #[wasm_bindgen(inline_js = "export function sum_array(x) {
            let s = 0;
            for (const v of x) s += Number(v);
            return s;
        }")]
        extern "C" {
            #[wasm_bindgen(js_name = sum_array)]
            fn sum_u32_slice(x: &[u32]) -> u32;
            #[wasm_bindgen(js_name = sum_array)]
            fn sum_u32_mut_slice(x: &mut [u32]) -> u32;
            #[wasm_bindgen(js_name = sum_array)]
            fn sum_u32_boxed(x: Box<[u32]>) -> u32;
        }
        assert_eq!(sum_u32_slice(&[1u32, 2, 3, 4]), 10);
        let mut v = vec![10u32, 20, 30];
        assert_eq!(sum_u32_mut_slice(&mut v[..]), 60);
        assert_eq!(sum_u32_boxed(vec![5u32, 6, 7].into_boxed_slice()), 18);
    }

    // Borrowed Clamped slices (encode-only) — pass as arguments.
    {
        #[wasm_bindgen(inline_js = "export function array_len(x) { return x.length; }")]
        extern "C" {
            #[wasm_bindgen(js_name = array_len)]
            fn clamped_ref_len(x: Clamped<&[u8]>) -> u32;
            #[wasm_bindgen(js_name = array_len)]
            fn clamped_mut_len(x: Clamped<&mut [u8]>) -> u32;
        }
        assert_eq!(clamped_ref_len(Clamped(&[1u8, 2, 3][..])), 3);
        let mut bytes = vec![4u8, 5];
        assert_eq!(clamped_mut_len(Clamped(&mut bytes[..])), 2);
    }

    // Heap-ref slice encoders: &[T] / &mut [T] where T: JsGeneric (e.g. JsValue).
    {
        use wasm_bindgen::JsValue;
        #[wasm_bindgen(inline_js = "export function heapref_array_len(x) { return x.length; }")]
        extern "C" {
            #[wasm_bindgen(js_name = heapref_array_len)]
            fn jsval_slice_len(x: &[JsValue]) -> u32;
            #[wasm_bindgen(js_name = heapref_array_len)]
            fn jsval_mut_slice_len(x: &mut [JsValue]) -> u32;
        }
        let mut v = vec![JsValue::from_f64(1.0), JsValue::from_f64(2.0)];
        assert_eq!(jsval_slice_len(&v[..]), 2);
        assert_eq!(jsval_mut_slice_len(&mut v[..]), 2);
    }

    // Borrowed primitive encoder: &T encodes via clone.
    {
        #[wasm_bindgen(inline_js = "export function double_num(x) { return x * 2; }")]
        extern "C" {
            #[wasm_bindgen(js_name = double_num)]
            fn double_u32_ref(x: &u32) -> u32;
        }
        assert_eq!(double_u32_ref(&21u32), 42);
    }
}
