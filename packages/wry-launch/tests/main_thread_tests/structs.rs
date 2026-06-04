use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "export function increment_by_5(s) {
    for (let i = 0; i < 5; i++)
        s.increment();
}
export function set_count(s, count) {
    s.count = count;
}
export function get_count(s) {
    return s.count;
}")]
extern "C" {
    fn increment_by_5(s: &JsValue);
    fn set_count(s: &JsValue, count: i32);
    fn get_count(s: &JsValue) -> i32;
}

#[wasm_bindgen(inline_js = r#"
export function call_counter_then_heap_ref(cb, counter) {
    cb(counter, { label: 123 });
}

export function heap_ref_label(obj) {
    return obj.label;
}
"#)]
extern "C" {
    fn call_counter_then_heap_ref(cb: &mut dyn FnMut(Counter, JsValue), counter: &JsValue);
    fn heap_ref_label(obj: &JsValue) -> u32;
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct Counter {
    count: i32,
}

#[wasm_bindgen]
impl Counter {
    #[wasm_bindgen(constructor)]
    pub fn new(count: i32) -> Counter {
        Counter { count }
    }

    #[wasm_bindgen(getter)]
    pub fn count(&self) -> i32 {
        self.count
    }

    #[wasm_bindgen(setter)]
    pub fn set_count(&mut self, count: i32) {
        self.count = count * 2;
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }
}

pub(crate) fn test_struct_bindings() {
    let counter = Counter::new(0);
    assert_eq!(counter.count(), 0);
    let as_js_value = JsValue::from(counter);
    increment_by_5(&as_js_value);
    assert_eq!(get_count(&as_js_value), 5);
    set_count(&as_js_value, 10);
    assert_eq!(get_count(&as_js_value), 20);
}

pub(crate) fn test_exported_struct_arg_before_heap_ref_arg() {
    let counter = JsValue::from(Counter::new(7));
    let mut called = false;

    call_counter_then_heap_ref(
        &mut |counter: Counter, value: JsValue| {
            assert_eq!(counter.count(), 7);
            assert_eq!(heap_ref_label(&value), 123);
            called = true;
        },
        &counter,
    );

    assert!(called, "callback was not called");
}
