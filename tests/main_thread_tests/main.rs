mod add_number_js;

fn main() {
    wry_testing::run(|| {
        add_number_js::test_add_number_js();
        add_number_js::test_add_number_js_batch();
    }).unwrap();
}
