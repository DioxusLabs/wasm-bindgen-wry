use wry_testing::set_on_log;

mod add_number_js;
mod callbacks;
mod jsvalue;
mod string_enum;

fn main() {
    wry_testing::run(|| {
        set_on_log(Box::new(|msg: String| {
            println!("[JS] {}", msg);
        }));

        // add_number_js::test_add_number_js();
        // add_number_js::test_add_number_js_batch();
        // callbacks::test_call_callback();
        // callbacks::test_call_callback_async();

        // // JsValue behavior tests
        // jsvalue::test_jsvalue_constants();
        // jsvalue::test_jsvalue_bool();
        // jsvalue::test_jsvalue_default();
        // jsvalue::test_jsvalue_clone_reserved();
        // jsvalue::test_jsvalue_equality();
        // jsvalue::test_jsvalue_from_js();
        // jsvalue::test_jsvalue_pass_to_js();
        jsvalue::test_jsvalue_as_string();

        // // String enum tests
        // string_enum::test_string_enum_from_str();
        // string_enum::test_string_enum_to_str();
        // string_enum::test_string_enum_to_jsvalue();
        // string_enum::test_string_enum_from_jsvalue();
    })
    .unwrap();
}
