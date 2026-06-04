//! Tests for string enum support

use wasm_bindgen::wasm_bindgen;

#[wasm_bindgen]
#[doc = "The `WebGlPowerPreference` enum."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebGlPowerPreference {
    Default = "default",
    LowPower = "low-power",
    HighPerformance = "high-performance",
}

pub fn test_string_enum_from_str() {
    assert_eq!(
        WebGlPowerPreference::from_str("default"),
        Some(WebGlPowerPreference::Default)
    );
    assert_eq!(
        WebGlPowerPreference::from_str("low-power"),
        Some(WebGlPowerPreference::LowPower)
    );
    assert_eq!(
        WebGlPowerPreference::from_str("high-performance"),
        Some(WebGlPowerPreference::HighPerformance)
    );
    assert_eq!(WebGlPowerPreference::from_str("invalid"), None);
}

pub fn test_string_enum_to_str() {
    assert_eq!(WebGlPowerPreference::Default.to_str(), "default");
    assert_eq!(WebGlPowerPreference::LowPower.to_str(), "low-power");
    assert_eq!(
        WebGlPowerPreference::HighPerformance.to_str(),
        "high-performance"
    );
}

pub fn test_string_enum_to_jsvalue() {
    use wasm_bindgen::JsValue;

    // Test converting enum to JsValue
    let pref = WebGlPowerPreference::Default;
    let js: JsValue = pref.into();

    // The JsValue should be a string "default"
    assert_eq!(js.as_string(), Some("default".to_string()));

    let pref2 = WebGlPowerPreference::HighPerformance;
    let js2: JsValue = pref2.into();
    assert_eq!(js2.as_string(), Some("high-performance".to_string()));
}

pub fn test_string_enum_from_jsvalue() {
    use wasm_bindgen::JsValue;

    // Create a JsValue string and convert back to enum
    let js = JsValue::from_str("low-power");
    let pref = WebGlPowerPreference::from_js_value(&js);
    assert_eq!(pref, Some(WebGlPowerPreference::LowPower));

    // Invalid string should return None
    let js_invalid = JsValue::from_str("not-a-valid-value");
    let pref_invalid = WebGlPowerPreference::from_js_value(&js_invalid);
    assert_eq!(pref_invalid, None);
}

// JavaScript function that accepts a string enum and returns its string value
#[wasm_bindgen(inline_js = "
export function get_string_enum_value(value) {
    // This should receive the string value, not a number
    if (typeof value !== 'string') {
        throw new Error('Expected string but got ' + typeof value + ': ' + value);
    }
    return value;
}
")]
extern "C" {
    fn get_string_enum_value(value: WebGlPowerPreference) -> String;
}

// Test that string enums are correctly passed to JavaScript as strings
pub fn test_string_enum_pass_to_js() {
    // Pass each variant and verify JS receives the correct string
    let result1 = get_string_enum_value(WebGlPowerPreference::Default);
    assert_eq!(result1, "default", "JS should receive 'default'");

    let result2 = get_string_enum_value(WebGlPowerPreference::LowPower);
    assert_eq!(result2, "low-power", "JS should receive 'low-power'");

    let result3 = get_string_enum_value(WebGlPowerPreference::HighPerformance);
    assert_eq!(
        result3, "high-performance",
        "JS should receive 'high-performance'"
    );
}

// JavaScript function that returns a string enum value
#[wasm_bindgen(inline_js = "
export function return_string_enum(index) {
    const values = ['default', 'low-power', 'high-performance'];
    return values[index];
}
")]
extern "C" {
    fn return_string_enum(index: u32) -> WebGlPowerPreference;
}

// Test that string enums are correctly received from JavaScript
pub fn test_string_enum_receive_from_js() {
    let result1 = return_string_enum(0);
    assert_eq!(result1, WebGlPowerPreference::Default);

    let result2 = return_string_enum(1);
    assert_eq!(result2, WebGlPowerPreference::LowPower);

    let result3 = return_string_enum(2);
    assert_eq!(result3, WebGlPowerPreference::HighPerformance);
}
