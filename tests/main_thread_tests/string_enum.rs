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
