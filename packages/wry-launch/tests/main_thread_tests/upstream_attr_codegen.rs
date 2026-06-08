use wasm_bindgen::{JsValue, wasm_bindgen};

#[wasm_bindgen(inline_js = r#"
export function read_codegen_attr_state() {
    return window.__wry_codegen_attr_state || "";
}

export function get_path(name) {
    return name.split(".").reduce((target, segment) => target && target[segment], window);
}

export function call_path_with_this(path, this_value, value) {
    return get_path(path).call(this_value, value);
}

export function call_path(path, value) {
    return get_path(path)(value);
}

export async function await_path(path, value) {
    return await get_path(path)(value);
}

export function construct_path(path, value) {
    return new (get_path(path))(value);
}

export async function await_construct_path(path, value) {
    return await new (get_path(path))(value);
}

export function is_instance_of_path(value, path) {
    return value instanceof get_path(path);
}

export function is_private_path_missing(path) {
    return get_path(path) === undefined;
}

export function read_property(value, name) {
    return value[name];
}

export function call_method(value, name) {
    return value[name]();
}

export async function await_method(value, name, arg) {
    return await value[name](arg);
}

export async function await_property(value, name) {
    return await value[name];
}

export function call_static_method(path, name) {
    return get_path(path)[name]();
}

export async function await_static_method(path, name, value) {
    return await get_path(path)[name](value);
}

export function call_reexported(name, a, b) {
    return window[name](a, b);
}

export function read_path_property(path, property) {
    return get_path(path)[property];
}
"#)]
extern "C" {
    fn read_codegen_attr_state() -> String;
    fn call_path_with_this(path: &str, this_value: &JsValue, value: u32) -> u32;
    #[wasm_bindgen(js_name = call_path)]
    fn call_path_i32(path: &str, value: i32) -> i32;
    #[wasm_bindgen(js_name = call_path)]
    fn call_path_value(path: &str, value: &JsValue) -> String;
    #[wasm_bindgen(js_name = call_path)]
    fn call_path_no_arg(path: &str) -> JsValue;
    async fn await_path(path: &str, value: u32) -> u32;
    fn construct_path(path: &str, value: u32) -> JsValue;
    async fn await_construct_path(path: &str, value: u32) -> JsValue;
    fn is_instance_of_path(value: &JsValue, path: &str) -> bool;
    fn is_private_path_missing(path: &str) -> bool;
    fn read_property(value: &JsValue, name: &str) -> u32;
    fn call_method(value: &JsValue, name: &str) -> u32;
    async fn await_method(value: &JsValue, name: &str) -> u32;
    #[wasm_bindgen(js_name = await_method)]
    async fn await_method_u32(value: &JsValue, name: &str, arg: u32) -> u32;
    #[wasm_bindgen(js_name = await_method)]
    async fn await_method_slice(value: &JsValue, name: &str, items: &[u32]) -> u32;
    async fn await_property(value: &JsValue, name: &str) -> u32;
    fn call_static_method(path: &str, name: &str) -> u32;
    async fn await_static_method(path: &str, name: &str, value: u32) -> u32;
    fn call_reexported(name: &str, a: u32, b: u32) -> u32;
    #[wasm_bindgen(js_name = get_path)]
    fn read_path_u32(name: &str) -> u32;
    #[wasm_bindgen(js_name = get_path)]
    fn read_path_string(name: &str) -> String;
    #[wasm_bindgen(js_name = read_path_property)]
    fn read_enum_value(path: &str, name: &str) -> i32;
    #[wasm_bindgen(js_name = read_path_property)]
    fn read_enum_name(path: &str, value: i32) -> String;
}

#[wasm_bindgen(inline_js = r#"
export function variadic_sum(first, second, ...rest) {
    return [first, second, ...rest].reduce((sum, value) => sum + value, 0);
}

export function imported_reexport_add(a, b) {
    return a + b;
}

export const imported_reexport_value = 271;

export function imported_union_value(kind) {
    return kind === "known" ? "known" : 12;
}

export const ImportedNs = {
    double(value) {
        return value * 2;
    },
    Nested: {
        triple(value) {
            return value * 3;
        },
    },
};
"#)]
#[rustfmt::skip]
extern "C" {
    #[wasm_bindgen(variadic)]
    fn variadic_sum(first: u32, second: u32, rest: &[u32]) -> u32;

    #[wasm_bindgen(reexport = "wry_codegen_reexport_add")]
    fn imported_reexport_add(a: u32, b: u32) -> u32;

    #[wasm_bindgen(
        thread_local_v2,
        reexport = "wry_codegen_reexport_value",
        js_name = imported_reexport_value
    )]
    static IMPORTED_REEXPORT_VALUE: u32;

    #[wasm_bindgen(js_namespace = ImportedNs, js_name = double)]
    fn imported_namespace_double(value: u32) -> u32;

    #[wasm_bindgen(js_namespace = ["ImportedNs", "Nested"], js_name = triple)]
    fn imported_nested_namespace_triple(value: u32) -> u32;

    fn imported_union_value(kind: &str) -> CodegenUnion;

    #[wasm_bindgen(thread_local_v2, static_string, reexport = "wry_codegen_static_string")]
    static CODEGEN_STATIC_STRING: String = "wry-static-string";
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = addTen)]
pub fn namespaced_export_add_ten(value: u32) -> u32 {
    value + 10
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = addThis, this)]
pub fn namespaced_export_add_this(this: &JsValue, value: u32) -> u32 {
    let current = js_sys::Reflect::get(this, &JsValue::from_str("count"))
        .unwrap()
        .as_f64()
        .unwrap() as u32;
    current + value
}

fn append_codegen_attr_state(segment: &str) {
    let current = js_sys::Reflect::get(
        &js_sys::global(),
        &JsValue::from_str("__wry_codegen_attr_state"),
    )
    .ok()
    .and_then(|value| value.as_string())
    .unwrap_or_default();
    js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("__wry_codegen_attr_state"),
        &JsValue::from_str(&format!("{current}{segment}")),
    )
    .unwrap();
}

#[wasm_bindgen(start)]
pub fn codegen_start_hook() {
    append_codegen_attr_state("|start");
}

#[wasm_bindgen(main)]
pub fn main() {
    append_codegen_attr_state("|main");
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "enums"])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodegenColor {
    Green,
    Yellow = 4,
    Red,
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "enums"], js_name = SignedKind)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodegenSignedKind {
    Negative = -2,
    Zero,
    Positive = 3,
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = cycleColor)]
pub fn cycle_color(color: CodegenColor) -> CodegenColor {
    match color {
        CodegenColor::Green => CodegenColor::Yellow,
        CodegenColor::Yellow => CodegenColor::Red,
        CodegenColor::Red => CodegenColor::Green,
    }
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = signedValue)]
pub fn signed_value(kind: CodegenSignedKind) -> i32 {
    kind as i32
}

#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodegenUnion {
    Known = "known",
    Count(u32),
}

#[wasm_bindgen]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodegenNestedUnion {
    Known = "nested-known",
    Inner(CodegenUnion),
}

#[wasm_bindgen(fallback)]
#[derive(Clone, Debug)]
pub enum CodegenFallbackUnion {
    Known = "known",
    Count(u32),
    Other(JsValue),
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = describeUnion)]
pub fn describe_union(value: CodegenUnion) -> String {
    match value {
        CodegenUnion::Known => "known".to_string(),
        CodegenUnion::Count(value) => format!("count:{value}"),
    }
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = describeNestedUnion)]
pub fn describe_nested_union(value: CodegenNestedUnion) -> String {
    match value {
        CodegenNestedUnion::Known => "nested-known".to_string(),
        CodegenNestedUnion::Inner(CodegenUnion::Known) => "inner:known".to_string(),
        CodegenNestedUnion::Inner(CodegenUnion::Count(value)) => format!("inner:count:{value}"),
    }
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = describeFallbackUnion)]
pub fn describe_fallback_union(value: CodegenFallbackUnion) -> String {
    match value {
        CodegenFallbackUnion::Known => "known".to_string(),
        CodegenFallbackUnion::Count(value) => format!("count:{value}"),
        CodegenFallbackUnion::Other(value) => {
            if value.is_array() {
                "other:array".to_string()
            } else {
                "other".to_string()
            }
        }
    }
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = makeKnownUnion)]
pub fn make_known_union() -> CodegenUnion {
    CodegenUnion::Known
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = makeCountUnion)]
pub fn make_count_union() -> CodegenUnion {
    CodegenUnion::Count(42)
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "exports"], js_name = asyncAddOne)]
pub async fn async_add_one(value: u32) -> u32 {
    value + 1
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "classes"], js_name = RenamedBase)]
pub struct CodegenBase {
    value: u32,
}

#[wasm_bindgen(js_class = RenamedBase, js_namespace = ["WryCodegen", "classes"])]
impl CodegenBase {
    #[wasm_bindgen(constructor)]
    pub fn new(value: u32) -> CodegenBase {
        CodegenBase { value }
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> u32 {
        self.value
    }

    pub fn double(&self) -> u32 {
        self.value * 2
    }

    pub fn static_marker() -> u32 {
        19
    }

    pub async fn async_double(&self) -> u32 {
        self.value * 2
    }

    pub async fn async_add_all(&self, values: &[u32]) -> u32 {
        self.value + values.iter().sum::<u32>()
    }

    pub async fn async_increment(&mut self, value: u32) -> u32 {
        self.value += value;
        self.value
    }

    #[wasm_bindgen(getter)]
    pub async fn async_value(&self) -> u32 {
        self.value
    }

    pub async fn async_static_marker(value: u32) -> u32 {
        value + 20
    }
}

#[wasm_bindgen(
    extends = CodegenBase,
    extends_js_class = RenamedBase,
    extends_js_namespace = ["WryCodegen", "classes"],
    js_namespace = ["WryCodegen", "classes"],
    js_name = RenamedChild
)]
pub struct CodegenChild {
    value: u32,
}

#[wasm_bindgen(js_class = RenamedChild, js_namespace = ["WryCodegen", "classes"])]
impl CodegenChild {
    #[wasm_bindgen(constructor)]
    pub fn new(value: u32) -> CodegenChild {
        CodegenChild {
            parent: CodegenBase::new(value).into(),
            value,
        }
    }

    pub fn triple(&self) -> u32 {
        self.value * 3
    }
}

#[wasm_bindgen(private, js_namespace = ["WryCodegen", "classes"], js_name = HiddenClass)]
pub struct HiddenCodegenClass {
    value: u32,
}

#[wasm_bindgen(js_class = HiddenClass, js_namespace = ["WryCodegen", "classes"])]
impl HiddenCodegenClass {
    #[wasm_bindgen(constructor)]
    pub fn new(value: u32) -> HiddenCodegenClass {
        HiddenCodegenClass { value }
    }

    pub fn value(&self) -> u32 {
        self.value
    }
}

#[wasm_bindgen(js_namespace = ["WryCodegen", "classes"], js_name = AsyncConstructed)]
pub struct CodegenAsyncConstructed {
    value: u32,
}

#[wasm_bindgen(js_class = AsyncConstructed, js_namespace = ["WryCodegen", "classes"])]
impl CodegenAsyncConstructed {
    #[wasm_bindgen(constructor)]
    pub async fn new(value: u32) -> CodegenAsyncConstructed {
        CodegenAsyncConstructed { value }
    }

    pub fn value(&self) -> u32 {
        self.value
    }
}

pub(crate) fn test_variadic_import_spreads_final_argument() {
    assert_eq!(variadic_sum(1, 2, &[]), 3);
    assert_eq!(variadic_sum(1, 2, &[3, 4]), 10);
}

pub(crate) fn test_imported_js_namespace_paths() {
    assert_eq!(imported_namespace_double(7), 14);
    assert_eq!(imported_nested_namespace_triple(7), 21);
}

pub(crate) fn test_reexport_installs_imported_values() {
    assert_eq!(imported_reexport_add(4, 5), 9);
    assert_eq!(call_reexported("wry_codegen_reexport_add", 8, 9), 17);
    IMPORTED_REEXPORT_VALUE.with(|value| assert_eq!(*value, 271));
    assert_eq!(read_path_u32("wry_codegen_reexport_value"), 271);
}

pub(crate) fn test_static_string_thread_local_and_reexport() {
    CODEGEN_STATIC_STRING.with(|value| assert_eq!(value, "wry-static-string"));
    assert_eq!(
        read_path_string("wry_codegen_static_string"),
        "wry-static-string"
    );
}

pub(crate) fn test_namespaced_export_and_this() {
    let this_value = js_sys::Object::new();
    js_sys::Reflect::set(
        &this_value,
        &JsValue::from_str("count"),
        &JsValue::from_f64(12.0),
    )
    .unwrap();
    let this_value: JsValue = this_value.into();

    assert_eq!(
        call_path_with_this("WryCodegen.exports.addTen", &JsValue::NULL, 32),
        42
    );
    assert_eq!(
        call_path_with_this("WryCodegen.exports.addThis", &this_value, 30),
        42
    );
}

pub(crate) fn test_start_export_runs_during_initialization() {
    let state = read_codegen_attr_state();
    assert!(state.contains("|start"), "{state}");
    assert!(state.contains("|main"), "{state}");
}

pub(crate) fn test_numeric_enums_export_and_roundtrip() {
    assert_eq!(read_enum_value("WryCodegen.enums.CodegenColor", "Green"), 0);
    assert_eq!(
        read_enum_value("WryCodegen.enums.CodegenColor", "Yellow"),
        4
    );
    assert_eq!(read_enum_name("WryCodegen.enums.CodegenColor", 5), "Red");
    assert_eq!(
        read_enum_value("WryCodegen.enums.SignedKind", "Negative"),
        -2
    );
    assert_eq!(read_enum_name("WryCodegen.enums.SignedKind", 3), "Positive");

    assert_eq!(
        call_path_with_this("WryCodegen.exports.cycleColor", &JsValue::NULL, 0),
        4
    );
    assert_eq!(call_path_i32("WryCodegen.exports.signedValue", -2), -2);
}

pub(crate) fn test_dynamic_union_export_argument_decode() {
    assert_eq!(
        call_path_value(
            "WryCodegen.exports.describeUnion",
            &JsValue::from_str("known")
        ),
        "known"
    );
    assert_eq!(
        call_path_value("WryCodegen.exports.describeUnion", &JsValue::from_f64(7.0)),
        "count:7"
    );
}

pub(crate) fn test_dynamic_union_import_return_decode() {
    assert_eq!(imported_union_value("known"), CodegenUnion::Known);
    assert_eq!(imported_union_value("count"), CodegenUnion::Count(12));
}

pub(crate) fn test_dynamic_union_nested_and_fallback() {
    assert_eq!(
        call_path_value(
            "WryCodegen.exports.describeNestedUnion",
            &JsValue::from_str("nested-known")
        ),
        "nested-known"
    );
    assert_eq!(
        call_path_value(
            "WryCodegen.exports.describeNestedUnion",
            &JsValue::from_f64(8.0)
        ),
        "inner:count:8"
    );

    assert_eq!(
        call_path_value(
            "WryCodegen.exports.describeFallbackUnion",
            &JsValue::from_str("known")
        ),
        "known"
    );
    assert_eq!(
        call_path_value(
            "WryCodegen.exports.describeFallbackUnion",
            &js_sys::Array::new().into()
        ),
        "other:array"
    );
}

pub(crate) fn test_dynamic_union_export_return_encode() {
    let known = call_path_no_arg("WryCodegen.exports.makeKnownUnion");
    assert_eq!(known.as_string().as_deref(), Some("known"));

    let count = call_path_no_arg("WryCodegen.exports.makeCountUnion");
    assert_eq!(count.as_f64(), Some(42.0));
}

pub(crate) fn test_exported_class_metadata_paths() {
    let base = construct_path("WryCodegen.classes.RenamedBase", 11);
    assert!(is_instance_of_path(&base, "WryCodegen.classes.RenamedBase"));
    assert_eq!(read_property(&base, "value"), 11);
    assert_eq!(call_method(&base, "double"), 22);
    assert_eq!(
        call_static_method("WryCodegen.classes.RenamedBase", "static_marker"),
        19
    );

    let child = construct_path("WryCodegen.classes.RenamedChild", 7);
    assert!(is_instance_of_path(
        &child,
        "WryCodegen.classes.RenamedChild"
    ));
    assert!(is_instance_of_path(
        &child,
        "WryCodegen.classes.RenamedBase"
    ));
    assert_eq!(call_method(&child, "triple"), 21);

    assert!(is_private_path_missing("WryCodegen.classes.HiddenClass"));
}

pub(crate) async fn test_async_export_returns_promise() {
    assert_eq!(await_path("WryCodegen.exports.asyncAddOne", 41).await, 42);
}

pub(crate) async fn test_async_receiver_methods_return_promise() {
    let base = construct_path("WryCodegen.classes.RenamedBase", 11);

    assert_eq!(await_method(&base, "async_double").await, 22);
    assert_eq!(
        await_method_slice(&base, "async_add_all", &[10, 11, 10]).await,
        42
    );
    assert_eq!(await_method_u32(&base, "async_increment", 5).await, 16);
    assert_eq!(await_property(&base, "async_value").await, 16);
}

pub(crate) async fn test_async_constructor_returns_instance_promise() {
    let value = await_construct_path("WryCodegen.classes.AsyncConstructed", 42).await;

    assert!(is_instance_of_path(
        &value,
        "WryCodegen.classes.AsyncConstructed"
    ));
    assert_eq!(call_method(&value, "value"), 42);
}

pub(crate) async fn test_async_static_method_returns_promise() {
    assert_eq!(
        await_static_method("WryCodegen.classes.RenamedBase", "async_static_marker", 22).await,
        42
    );
}
