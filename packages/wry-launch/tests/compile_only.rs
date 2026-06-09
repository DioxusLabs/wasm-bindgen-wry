use wasm_bindgen::wasm_bindgen;

#[test]
fn colon_module_specifier_compiles_as_raw_import() {
    #[wasm_bindgen(module = "cloudflare:sockets")]
    extern "C" {
        fn __wry_worker_sys_socket_marker();
    }

    let _ = __wry_worker_sys_socket_marker as fn();
}

#[test]
fn rust_2015_extern_type_expansion_compiles() {
    wry_launch_rust2015_extern_type::touch();
}
