//! Regression tests: a type alias that hides a reference/slice/struct argument
//! must behave identically to the spelled-out type.
//!
//! These exercise the `ArgAbi` projection. The export macro keys argument decode
//! on the *full spelled* type — `<#arg_ty as ArgAbi>::decode/project/write_back`
//! — and lets trait resolution see through the alias. Before that change an
//! aliased borrowed argument fell into the owned-decode arm (`<&[u8] as
//! BinaryDecode>`, which has no impl) and the export failed to compile, so the
//! mere existence-and-passing of these tests is the regression proof for the
//! export side. The import and return cases are controls: those paths were
//! already trait-driven (`BinaryEncode` / `ReturnWasmAbi`) and alias-transparent.

use wasm_bindgen::{JsValue, wasm_bindgen};

// Aliases that hide the reference/slice/struct shape from the proc-macro.
type U8Slice<'a> = &'a [u8];
type U8SliceMut<'a> = &'a mut [u8];
type StrAlias<'a> = &'a str;
type StructRef<'a> = &'a AliasFixture;
type MyString = String;
type MyResult = Result<i32, JsValue>;

#[wasm_bindgen(inline_js = r#"
export function drive_sum_direct() { return window.aliasSumDirect(new Uint8Array([1, 2, 3, 4])); }
export function drive_sum_alias() { return window.aliasSumAlias(new Uint8Array([1, 2, 3, 4])); }

export function drive_concat_direct() { return window.aliasConcatDirect("ab"); }
export function drive_concat_alias() { return window.aliasConcatAlias("ab"); }

export function drive_fill_direct() {
    const v = new Uint8Array([0, 0, 0]);
    window.aliasFillDirect(v);
    return Array.from(v).join(",");
}
export function drive_fill_alias() {
    const v = new Uint8Array([0, 0, 0]);
    window.aliasFillAlias(v);
    return Array.from(v).join(",");
}

export function drive_struct_direct() {
    const f = new window.AliasFixture(7);
    return window.aliasReadStructDirect(f);
}
export function drive_struct_alias() {
    const f = new window.AliasFixture(7);
    return window.aliasReadStructAlias(f);
}

export function import_sum(values) { let s = 0; for (const v of values) s += Number(v); return s; }

export function drive_ret_string_direct() { return window.aliasRetStringDirect(); }
export function drive_ret_string_alias() { return window.aliasRetStringAlias(); }
export function drive_ret_result_direct() { return window.aliasRetResultDirect(); }
export function drive_ret_result_alias() { return window.aliasRetResultAlias(); }
"#)]
extern "C" {
    fn drive_sum_direct() -> u32;
    fn drive_sum_alias() -> u32;
    fn drive_concat_direct() -> String;
    fn drive_concat_alias() -> String;
    fn drive_fill_direct() -> String;
    fn drive_fill_alias() -> String;
    fn drive_struct_direct() -> u32;
    fn drive_struct_alias() -> u32;
    // Import-side control: both spellings call the same JS `import_sum`.
    #[wasm_bindgen(js_name = import_sum)]
    fn import_sum_direct(values: &[u8]) -> u32;
    #[wasm_bindgen(js_name = import_sum)]
    fn import_sum_alias(values: U8Slice) -> u32;
    fn drive_ret_string_direct() -> String;
    fn drive_ret_string_alias() -> String;
    fn drive_ret_result_direct() -> i32;
    fn drive_ret_result_alias() -> i32;
}

// --- Exports: each pair is the direct spelling vs the alias spelling. ---

#[wasm_bindgen(js_name = aliasSumDirect)]
pub fn alias_sum_direct(values: &[u8]) -> u32 {
    values.iter().map(|&b| b as u32).sum()
}
#[wasm_bindgen(js_name = aliasSumAlias)]
pub fn alias_sum_alias(values: U8Slice) -> u32 {
    values.iter().map(|&b| b as u32).sum()
}

#[wasm_bindgen(js_name = aliasConcatDirect)]
pub fn alias_concat_direct(s: &str) -> String {
    format!("{s}{s}")
}
#[wasm_bindgen(js_name = aliasConcatAlias)]
pub fn alias_concat_alias(s: StrAlias) -> String {
    format!("{s}{s}")
}

#[wasm_bindgen(js_name = aliasFillDirect)]
pub fn alias_fill_direct(values: &mut [u8]) {
    for (i, v) in values.iter_mut().enumerate() {
        *v = i as u8 + 1;
    }
}
#[wasm_bindgen(js_name = aliasFillAlias)]
pub fn alias_fill_alias(values: U8SliceMut) {
    for (i, v) in values.iter_mut().enumerate() {
        *v = i as u8 + 1;
    }
}

#[wasm_bindgen]
pub struct AliasFixture {
    val: u32,
}
#[wasm_bindgen]
impl AliasFixture {
    #[wasm_bindgen(constructor)]
    pub fn new(val: u32) -> AliasFixture {
        AliasFixture { val }
    }
}

#[wasm_bindgen(js_name = aliasReadStructDirect)]
pub fn alias_read_struct_direct(f: &AliasFixture) -> u32 {
    f.val
}
#[wasm_bindgen(js_name = aliasReadStructAlias)]
pub fn alias_read_struct_alias(f: StructRef) -> u32 {
    f.val
}

#[wasm_bindgen(js_name = aliasRetStringDirect)]
pub fn alias_ret_string_direct() -> String {
    "hi".to_string()
}
#[wasm_bindgen(js_name = aliasRetStringAlias)]
pub fn alias_ret_string_alias() -> MyString {
    "hi".to_string()
}

#[wasm_bindgen(js_name = aliasRetResultDirect)]
pub fn alias_ret_result_direct() -> Result<i32, JsValue> {
    Ok(5)
}
#[wasm_bindgen(js_name = aliasRetResultAlias)]
pub fn alias_ret_result_alias() -> MyResult {
    Ok(5)
}

// --- Tests ---

pub(crate) fn test_export_slice_arg_alias() {
    assert_eq!(drive_sum_direct(), 10);
    assert_eq!(
        drive_sum_alias(),
        10,
        "aliased &[u8] export arg must match the direct spelling"
    );
}

pub(crate) fn test_export_str_arg_alias() {
    assert_eq!(drive_concat_direct(), "abab");
    assert_eq!(
        drive_concat_alias(),
        "abab",
        "aliased &str export arg must match the direct spelling"
    );
}

pub(crate) fn test_export_mut_slice_arg_alias() {
    assert_eq!(drive_fill_direct(), "1,2,3");
    assert_eq!(
        drive_fill_alias(),
        "1,2,3",
        "aliased &mut [u8] export arg must write back like the direct spelling"
    );
}

pub(crate) fn test_export_struct_ref_arg_alias() {
    assert_eq!(drive_struct_direct(), 7);
    assert_eq!(
        drive_struct_alias(),
        7,
        "aliased &Struct export arg must match the direct spelling"
    );
}

pub(crate) fn test_import_slice_arg_alias() {
    assert_eq!(import_sum_direct(&[1, 2, 3, 4]), 10);
    assert_eq!(
        import_sum_alias(&[1, 2, 3, 4]),
        10,
        "aliased &[u8] import arg must match the direct spelling"
    );
}

pub(crate) fn test_return_type_alias() {
    assert_eq!(drive_ret_string_direct(), "hi");
    assert_eq!(
        drive_ret_string_alias(),
        "hi",
        "aliased String return must match the direct spelling"
    );
    assert_eq!(drive_ret_result_direct(), 5);
    assert_eq!(
        drive_ret_result_alias(),
        5,
        "aliased Result return must match the direct spelling"
    );
}
