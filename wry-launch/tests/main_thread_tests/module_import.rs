use wasm_bindgen::{JsCast, wasm_bindgen};

pub(crate) fn test_module_import() {
    #[wasm_bindgen(module = "/tests/main_thread_tests/test_module.js")]
    extern "C" {
        fn multiply(a: u32, b: u32) -> u32;
    }

    let result = multiply(3, 4);
    assert_eq!(result, 12);

    #[wasm_bindgen(inline_js = r#"
        export function add_one(value) {
            return value + 1;
        }

        export function double(value) {
            return value * 2;
        }

        export class ModuleThing {
            constructor(value) {
                this.value = value;
            }

            get_value() {
                return this.value;
            }
        }
    "#)]
    extern "C" {
        fn add_one(value: u32) -> u32;
        fn double(value: u32) -> u32;

        type ModuleThing;

        #[wasm_bindgen(constructor)]
        fn new(value: u32) -> ModuleThing;

        #[wasm_bindgen(method, js_name = get_value)]
        fn get_value(this: &ModuleThing) -> u32;
    }

    assert_eq!(add_one(4), 5);
    assert_eq!(double(6), 12);

    let thing = ModuleThing::new(42);
    assert_eq!(thing.get_value(), 42);
    assert!(thing.is_instance_of::<ModuleThing>());

    #[wasm_bindgen(inline_js = r#"
        export class FreeOnlyThing {
            constructor(value) {
                this.value = value;
            }

            get_value() {
                return this.value;
            }
        }

        export class RenamedModuleThing {
            constructor(value) {
                this.value = value;
            }

            get_value() {
                return this.value;
            }
        }

        export function make_free_only_thing(value) {
            return new FreeOnlyThing(value);
        }

        export function make_renamed_module_thing(value) {
            return new RenamedModuleThing(value);
        }
    "#)]
    extern "C" {
        type FreeOnlyThing;

        #[wasm_bindgen(js_name = RenamedModuleThing)]
        type RustNamedModuleThing;

        fn make_free_only_thing(value: u32) -> FreeOnlyThing;
        fn make_renamed_module_thing(value: u32) -> RustNamedModuleThing;

        #[wasm_bindgen(method, js_name = get_value)]
        fn get_free_only_value(this: &FreeOnlyThing) -> u32;

        #[wasm_bindgen(method, js_name = get_value)]
        fn get_renamed_value(this: &RustNamedModuleThing) -> u32;
    }

    let free_only = make_free_only_thing(7);
    assert_eq!(free_only.get_free_only_value(), 7);
    assert!(free_only.is_instance_of::<FreeOnlyThing>());

    let renamed = make_renamed_module_thing(13);
    assert_eq!(renamed.get_renamed_value(), 13);
    assert!(renamed.is_instance_of::<RustNamedModuleThing>());
}
