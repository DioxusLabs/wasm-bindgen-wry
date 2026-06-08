use wasm_bindgen::wasm_bindgen;
use wry_launch::batch;

#[wasm_bindgen(inline_js = r#"
let thunkCalls = 0;

export function reset_thunk_calls() {
    thunkCalls = 0;
}

export function js_thunk() {
    thunkCalls += 1;
}

export function thunk_calls() {
    return thunkCalls;
}

export function call_exported_js_thunk_benchmark(n) {
    return window.JsThunkExportFixture.callJsThunkNTimes(n);
}

export function call_exported_js_thunk_benchmark_batched(n) {
    return window.JsThunkExportFixture.callJsThunkNTimesBatched(n);
}

function as_list(values) {
    return Array.from(values).join(",");
}

export function call_unit_write_back_free() {
    const values = new Uint32Array([1, 2, 3]);
    window.unitWriteBackFree(values);
    return as_list(values);
}

export function call_returning_write_back_free() {
    const values = new Uint32Array([1, 2, 3]);
    const ret = window.returningWriteBackFree(values);
    return `${ret}:${as_list(values)}`;
}

export function call_unit_write_back_constructor() {
    const values = new Uint32Array([1, 2, 3]);
    const fixture = new window.UnitWriteBackFixture(values);
    return `${fixture.value}:${as_list(values)}`;
}

export function call_unit_write_back_static() {
    const values = new Uint32Array([1, 2, 3]);
    window.UnitWriteBackFixture.unitStatic(values);
    return as_list(values);
}

export function call_unit_write_back_method() {
    const fixture = new window.UnitWriteBackFixture(new Uint32Array([0, 0, 0]));
    const values = new Uint32Array([1, 2, 3]);
    fixture.unitMethod(values);
    return `${fixture.value}:${as_list(values)}`;
}

export function call_unit_write_back_setter() {
    const fixture = new window.UnitWriteBackFixture(new Uint32Array([0, 0, 0]));
    const values = new Uint32Array([1, 2, 3]);
    fixture.items = values;
    return `${fixture.value}:${as_list(values)}`;
}
	"#)]
extern "C" {
    fn reset_thunk_calls();

    #[wasm_bindgen(js_name = js_thunk)]
    fn js_thunk();

    fn thunk_calls() -> u32;

    fn call_exported_js_thunk_benchmark(n: u32) -> u32;

    fn call_exported_js_thunk_benchmark_batched(n: u32) -> u32;

    fn call_unit_write_back_free() -> String;

    fn call_returning_write_back_free() -> String;

    fn call_unit_write_back_constructor() -> String;

    fn call_unit_write_back_static() -> String;

    fn call_unit_write_back_method() -> String;

    fn call_unit_write_back_setter() -> String;
}

#[wasm_bindgen]
pub struct JsThunkExportFixture;

#[wasm_bindgen]
impl JsThunkExportFixture {
    #[wasm_bindgen(js_name = callJsThunkNTimes)]
    pub fn call_js_thunk_n_times(n: usize) -> u32 {
        for _ in 0..n {
            js_thunk();
        }
        n as u32
    }

    #[wasm_bindgen(js_name = callJsThunkNTimesBatched)]
    pub fn call_js_thunk_n_times_batched(n: usize) -> u32 {
        batch(|| {
            for _ in 0..n {
                js_thunk();
            }
        });
        n as u32
    }
}

#[wasm_bindgen(js_name = unitWriteBackFree)]
pub fn unit_write_back_free(values: &mut [u32]) {
    values[0] = 11;
    values[1] = 12;
}

#[wasm_bindgen(js_name = returningWriteBackFree)]
pub fn returning_write_back_free(values: &mut [u32]) -> u32 {
    values[0] = 21;
    values[1] = 22;
    77
}

#[wasm_bindgen]
pub struct UnitWriteBackFixture {
    value: u32,
}

#[wasm_bindgen]
impl UnitWriteBackFixture {
    #[wasm_bindgen(constructor)]
    pub fn new(values: &mut [u32]) -> UnitWriteBackFixture {
        values[0] = 31;
        values[1] = 32;
        UnitWriteBackFixture { value: 300 }
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> u32 {
        self.value
    }

    #[wasm_bindgen(js_name = unitStatic)]
    pub fn unit_static(values: &mut [u32]) {
        values[0] = 41;
        values[1] = 42;
    }

    #[wasm_bindgen(js_name = unitMethod)]
    pub fn unit_method(&mut self, values: &mut [u32]) {
        values[0] = 51;
        values[1] = 52;
        self.value = 500;
    }

    #[wasm_bindgen(setter, js_name = items)]
    pub fn set_items(&mut self, values: &mut [u32]) {
        values[0] = 61;
        values[1] = 62;
        self.value = 600;
    }
}

pub(crate) fn test_js_calls_exported_usize_js_thunk() {
    reset_thunk_calls();

    let result = call_exported_js_thunk_benchmark(3);

    assert_eq!(result, 3);
    assert_eq!(thunk_calls(), 3);
}

pub(crate) fn test_js_calls_exported_usize_js_thunk_batched() {
    reset_thunk_calls();

    let result = call_exported_js_thunk_benchmark_batched(3);

    assert_eq!(result, 3);
    assert_eq!(thunk_calls(), 3);
}

pub(crate) fn test_unit_export_write_back_free_function() {
    assert_eq!(call_unit_write_back_free(), "11,12,3");
}

pub(crate) fn test_returning_export_write_back_order() {
    assert_eq!(call_returning_write_back_free(), "77:21,22,3");
}

pub(crate) fn test_unit_export_write_back_constructor() {
    assert_eq!(call_unit_write_back_constructor(), "300:31,32,3");
}

pub(crate) fn test_unit_export_write_back_static_method() {
    assert_eq!(call_unit_write_back_static(), "41,42,3");
}

pub(crate) fn test_unit_export_write_back_instance_method() {
    assert_eq!(call_unit_write_back_method(), "500:51,52,3");
}

pub(crate) fn test_unit_export_write_back_setter() {
    assert_eq!(call_unit_write_back_setter(), "600:61,62,3");
}
