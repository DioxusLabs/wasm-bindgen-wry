use wasm_bindgen::{
    JsCast, JsError, JsValue,
    convert::{
        FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi, OptionIntoWasmAbi, RefFromWasmAbi,
        TryFromJsValue, WasmAbi,
    },
    sys::{JsOption, Promising},
    wasm_bindgen,
};

#[wasm_bindgen]
extern "C" {
    type DefaultPromisingCompatType;

    #[wasm_bindgen(no_promising)]
    type ManualPromisingCompatType;
}

impl Promising for ManualPromisingCompatType {
    type Resolution = JsValue;
}

pub(crate) fn test_imported_type_promising_compat() {
    fn assert_default<T: Promising<Resolution = T>>() {}
    fn assert_manual<T: Promising<Resolution = JsValue>>() {}

    assert_default::<DefaultPromisingCompatType>();
    assert_manual::<ManualPromisingCompatType>();
}

pub(crate) fn test_generic_import_erases_promise_method_shape() {
    #[wasm_bindgen]
    extern "C" {
        type CompatPromise<T = JsValue>;

        #[wasm_bindgen(method, js_name = then)]
        fn compat_then_map<'a, T, R: Promising>(
            this: &CompatPromise<T>,
            cb: &wasm_bindgen::ScopedClosure<'a, dyn FnMut(T) -> Result<R, JsError>>,
        ) -> CompatPromise<R::Resolution>;
    }

    let _ = CompatPromise::<JsValue>::compat_then_map::<JsValue>;
}

pub(crate) fn test_convert_traits_are_marker_bounds() {
    fn assert_into<T: IntoWasmAbi>() {}
    fn assert_from<T: FromWasmAbi>() {}
    fn assert_option_into<T: OptionIntoWasmAbi>() {}
    fn assert_option_from<T: OptionFromWasmAbi>() {}
    fn assert_wasm<T: WasmAbi>() {}
    fn assert_ref<T: RefFromWasmAbi>() {}
    // Matches wasm-bindgen: `JsError` is *not* itself a `std::error::Error`, but it does
    // implement `From<E> where E: std::error::Error` (so `?` works on `Result<_, JsError>`).
    fn assert_jserror_from<E: std::error::Error>()
    where
        JsError: From<E>,
    {
    }

    assert_into::<JsValue>();
    assert_from::<JsValue>();
    assert_ref::<JsValue>();
    assert_wasm::<JsValue>();

    assert_into::<Option<u32>>();
    assert_from::<Option<u32>>();
    assert_option_into::<Option<u32>>();
    assert_option_from::<Option<u32>>();
    assert_wasm::<Option<u32>>();

    assert_jserror_from::<std::io::Error>();
}

pub(crate) fn test_interned_string_roundtrip() {
    #[wasm_bindgen(inline_js = r#"
        export function api_echo_string(value) {
            return value;
        }
    "#)]
    extern "C" {
        fn api_echo_string(value: &str) -> String;
    }

    let interned = wasm_bindgen::intern("cached string");
    assert_eq!(api_echo_string(interned), "cached string");
    wasm_bindgen::unintern("cached string");
    assert_eq!(api_echo_string("cached string"), "cached string");
}

pub(crate) fn test_jsvalue_abi_ref_preserves_heap_ref() {
    #[wasm_bindgen(inline_js = r#"
        export function api_abi_object(label) {
            return { label };
        }

        export function api_abi_read_label(value) {
            return value.label;
        }
    "#)]
    extern "C" {
        fn api_abi_object(label: u32) -> JsValue;
        fn api_abi_read_label(value: &JsValue) -> u32;
    }

    let value = api_abi_object(271);
    let abi = (&value).into_abi();
    let cloned = unsafe { JsValue::ref_from_abi(abi) }.as_ref().clone();
    assert_eq!(api_abi_read_label(&value), 271);
    assert_eq!(api_abi_read_label(&cloned), 271);

    let owned = api_abi_object(314);
    let abi = owned.into_abi();
    let decoded = unsafe { JsValue::from_abi(abi) };
    assert_eq!(api_abi_read_label(&decoded), 314);
}

pub(crate) fn test_u64_try_from_bigint_preserves_range() {
    #[wasm_bindgen(inline_js = r#"
        export function api_u64_max_bigint() {
            return (1n << 64n) - 1n;
        }

        export function api_u64_negative_bigint() {
            return -1n;
        }

        export function api_u64_too_large_bigint() {
            return 1n << 64n;
        }
    "#)]
    extern "C" {
        fn api_u64_max_bigint() -> JsValue;
        fn api_u64_negative_bigint() -> JsValue;
        fn api_u64_too_large_bigint() -> JsValue;
    }

    assert_eq!(u64::try_from(api_u64_max_bigint()).unwrap(), u64::MAX);
    assert!(u64::try_from(api_u64_negative_bigint()).is_err());
    assert!(u64::try_from(api_u64_too_large_bigint()).is_err());
}

pub(crate) fn test_u128_try_from_bigint_preserves_range() {
    #[wasm_bindgen(inline_js = r#"
        export function api_u128_large_bigint() {
            return 1n << 80n;
        }

        export function api_u128_negative_bigint() {
            return -1n;
        }

        export function api_u128_too_large_bigint() {
            return 1n << 128n;
        }
    "#)]
    extern "C" {
        fn api_u128_large_bigint() -> JsValue;
        fn api_u128_negative_bigint() -> JsValue;
        fn api_u128_too_large_bigint() -> JsValue;
    }

    assert_eq!(
        u128::try_from(api_u128_large_bigint()).unwrap(),
        1_u128 << 80
    );
    assert!(u128::try_from(api_u128_negative_bigint()).is_err());
    assert!(u128::try_from(api_u128_too_large_bigint()).is_err());
}

pub(crate) fn test_i128_try_from_bigint_preserves_full_width() {
    #[wasm_bindgen(inline_js = r#"
        export function api_i128_large_positive_bigint() {
            return 1n << 80n;
        }

        export function api_i128_large_negative_bigint() {
            return -(1n << 80n);
        }

        export function api_i128_min_bigint() {
            return -(1n << 127n);
        }

        export function api_i128_too_large_bigint() {
            return 1n << 127n;
        }

        export function api_i128_too_negative_bigint() {
            return -(1n << 127n) - 1n;
        }
    "#)]
    extern "C" {
        fn api_i128_large_positive_bigint() -> JsValue;
        fn api_i128_large_negative_bigint() -> JsValue;
        fn api_i128_min_bigint() -> JsValue;
        fn api_i128_too_large_bigint() -> JsValue;
        fn api_i128_too_negative_bigint() -> JsValue;
    }

    assert_eq!(
        i128::try_from(api_i128_large_positive_bigint()).unwrap(),
        1_i128 << 80
    );
    assert_eq!(
        i128::try_from(api_i128_large_negative_bigint()).unwrap(),
        -(1_i128 << 80)
    );
    assert_eq!(i128::try_from(api_i128_min_bigint()).unwrap(), i128::MIN);
    assert!(i128::try_from(api_i128_too_large_bigint()).is_err());
    assert!(i128::try_from(api_i128_too_negative_bigint()).is_err());
}

pub(crate) fn test_i64_try_from_bigint_preserves_precision_above_f64() {
    #[wasm_bindgen(inline_js = r#"
        export function api_i64_2_pow_53_plus_one() {
            return (1n << 53n) + 1n;
        }

        export function api_i64_max_bigint() {
            return (1n << 63n) - 1n;
        }

        export function api_i64_min_bigint() {
            return -(1n << 63n);
        }

        export function api_i64_large_negative_bigint() {
            return -((1n << 53n) + 1n);
        }
    "#)]
    extern "C" {
        fn api_i64_2_pow_53_plus_one() -> JsValue;
        fn api_i64_max_bigint() -> JsValue;
        fn api_i64_min_bigint() -> JsValue;
        fn api_i64_large_negative_bigint() -> JsValue;
    }

    // 2^53 + 1 is the smallest integer an `f64` cannot represent. The old
    // `Number(BigInt.asIntN(64, x))` path silently rounded it down to 2^53;
    // encoding the bigint directly preserves every bit.
    assert_eq!(
        <i64 as TryFromJsValue>::try_from_js_value(api_i64_2_pow_53_plus_one()).unwrap(),
        (1_i64 << 53) + 1
    );
    assert_eq!(
        <i64 as TryFromJsValue>::try_from_js_value(api_i64_max_bigint()).unwrap(),
        i64::MAX
    );
    assert_eq!(
        <i64 as TryFromJsValue>::try_from_js_value(api_i64_min_bigint()).unwrap(),
        i64::MIN
    );
    assert_eq!(
        <i64 as TryFromJsValue>::try_from_js_value(api_i64_large_negative_bigint()).unwrap(),
        -((1_i64 << 53) + 1)
    );
}

pub(crate) fn test_try_from_js_value_signed_numbers_preserve_negative_values() {
    #[wasm_bindgen(inline_js = r#"
        export function api_negative_one_number() {
            return -1;
        }

        export function api_i8_min_number() {
            return -128;
        }

        export function api_i16_min_number() {
            return -32768;
        }

        export function api_signed_i32_array() {
            return [-1, -2, -2147483648, 2147483647];
        }
    "#)]
    extern "C" {
        fn api_negative_one_number() -> JsValue;
        fn api_i8_min_number() -> JsValue;
        fn api_i16_min_number() -> JsValue;
        fn api_signed_i32_array() -> JsValue;
    }

    assert_eq!(
        <i8 as TryFromJsValue>::try_from_js_value(api_negative_one_number()).unwrap(),
        -1
    );
    assert_eq!(
        <i8 as TryFromJsValue>::try_from_js_value(api_i8_min_number()).unwrap(),
        i8::MIN
    );
    assert_eq!(
        <i16 as TryFromJsValue>::try_from_js_value(api_i16_min_number()).unwrap(),
        i16::MIN
    );
    assert_eq!(
        <i32 as TryFromJsValue>::try_from_js_value(api_negative_one_number()).unwrap(),
        -1
    );
    assert_eq!(
        <Vec<i32> as TryFromJsValue>::try_from_js_value(api_signed_i32_array()).unwrap(),
        vec![-1, -2, i32::MIN, i32::MAX]
    );
}
