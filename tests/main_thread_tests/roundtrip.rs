use wasm_bindgen::wasm_bindgen;

pub(crate) fn test_roundtrip() {
    macro_rules! roundtrip {
        ($t:ty, $val:expr) => {
            {
                println!("testing roundtrip for type {}", stringify!($t));
                #[wasm_bindgen(inline_js = "export function identity(x) { return x; }")]
                extern "C" {
                    #[wasm_bindgen(js_name = identity)]
                    fn identity(x: $t) -> $t;
                }
    
                let input: $t = $val;
                let output: $t = identity(input.clone());
                assert_eq!(input, output, "Roundtrip failed for type {}", stringify!($t));
            }
        };
    }

    roundtrip!(u8, 42u8);
    roundtrip!(u16, 42u16);
    roundtrip!(u32, 42u32);
    roundtrip!(u64, 42u64);
    roundtrip!(i8, -42i8);
    roundtrip!(i16, -42i16);
    roundtrip!(i32, -42i32);
    roundtrip!(i64, -42i64);
    roundtrip!(f32, 3.14159f32);
    roundtrip!(f64, 3.14159f64);
    roundtrip!(String, "Hello, world!".to_string());
    roundtrip!(bool, true);
    roundtrip!(bool, false);
    roundtrip!(Option<u32>, Some(100u32));
    roundtrip!(Option<u32>, None);
}
