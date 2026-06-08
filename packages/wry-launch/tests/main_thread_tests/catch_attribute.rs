use wasm_bindgen::JsValue;
use wasm_bindgen::wasm_bindgen;

pub(crate) fn test_catch_throws_error() {
    #[wasm_bindgen(inline_js = "export function throws_error() { throw new Error('test error'); }")]
    extern "C" {
        #[wasm_bindgen(js_name = throws_error, catch)]
        fn throws_error() -> Result<(), JsValue>;
    }

    let result = throws_error();
    assert!(result.is_err(), "Expected error from throwing function");

    if let Err(e) = result {
        let error_string = format!("{e:?}");
        println!("Caught error: {error_string}");
        assert!(
            error_string.contains("test error") || error_string.contains("Error"),
            "Error should contain error message"
        );
    }
}

pub(crate) fn test_catch_successful_call() {
    #[wasm_bindgen(inline_js = "export function succeeds() { return 42; }")]
    extern "C" {
        #[wasm_bindgen(js_name = succeeds, catch)]
        fn succeeds() -> Result<u32, JsValue>;
    }

    let result = succeeds();
    assert!(result.is_ok(), "Expected successful result");
    assert_eq!(result.unwrap(), 42);
}

pub(crate) fn test_catch_with_arguments() {
    #[wasm_bindgen(inline_js = r#"
        export function divide(a, b) {
            if (b === 0) {
                throw new Error('Division by zero');
            }
            return a / b;
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = divide, catch)]
        fn divide(a: f64, b: f64) -> Result<f64, JsValue>;
    }

    // Test successful division
    let result = divide(10.0, 2.0);
    assert!(result.is_ok(), "Expected successful division");
    assert_eq!(result.unwrap(), 5.0);

    // Test division by zero
    let result = divide(10.0, 0.0);
    assert!(result.is_err(), "Expected error from division by zero");
}

pub(crate) fn test_catch_method() {
    #[wasm_bindgen(inline_js = r#"
        export class Calculator {
            constructor() {}

            divide(a, b) {
                if (b === 0) {
                    throw new Error('Cannot divide by zero');
                }
                return a / b;
            }
        }
    "#)]
    extern "C" {
        #[wasm_bindgen]
        type Calculator;

        #[wasm_bindgen(constructor)]
        fn new() -> Calculator;

        #[wasm_bindgen(method, catch)]
        fn divide(this: &Calculator, a: f64, b: f64) -> Result<f64, JsValue>;
    }

    let calc = Calculator::new();

    // Test successful call
    let result = calc.divide(20.0, 4.0);
    assert!(result.is_ok(), "Expected successful method call");
    assert_eq!(result.unwrap(), 5.0);

    // Test error case
    let result = calc.divide(10.0, 0.0);
    assert!(result.is_err(), "Expected error from method");
}

// A `Result` return type reached through an alias whose name is not `Result`.
// The export still throws its `Err` in JS, because the wire type is resolved by
// `ReturnWasmAbi` (after the alias is normalized) rather than by matching the
// spelling `Result` in the macro.
type FallibleAlias = Result<u32, JsValue>;

#[wasm_bindgen]
pub struct AliasFixture;

#[wasm_bindgen]
impl AliasFixture {
    #[wasm_bindgen(js_name = fallible)]
    pub fn fallible(fail: bool) -> FallibleAlias {
        if fail {
            Err(JsValue::from_str("aliased boom"))
        } else {
            Ok(7)
        }
    }
}

pub(crate) fn test_result_alias_export_throws() {
    // `fallible` returns the `Ok` value directly; on `Err` the export call
    // throws, so the catch path runs and returns the sentinel `99`. With the old
    // name-matching codegen the aliased return would have been a non-throwing
    // `{ ok, err }` value, so the catch would never run.
    #[wasm_bindgen(inline_js = r#"
        export function call_aliased_fallible(fail) {
            try {
                return window.AliasFixture.fallible(fail);
            } catch (e) {
                return 99;
            }
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = call_aliased_fallible)]
        fn call_aliased_fallible(fail: bool) -> u32;
    }

    // `Ok` returns the value (no throw).
    assert_eq!(call_aliased_fallible(false), 7);

    // `Err` is thrown in JS rather than returned as a value, even though the
    // return type is spelled via an alias rather than `Result`.
    assert_eq!(
        call_aliased_fallible(true),
        99,
        "expected the aliased Result export to throw on Err"
    );
}
