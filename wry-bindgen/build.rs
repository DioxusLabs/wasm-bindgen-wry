fn compile_ts() {
    // If any TS files change, re-run the build script
    lazy_js_bundle::LazyTypeScriptBindings::new()
        .with_watching("./src/ts")
        .with_binding("./src/ts/convert.ts", "./src/js/convert.js")
        .with_binding("./src/ts/main.ts", "./src/js/main.js")
        .run();
}

fn main() {
    compile_ts();
}
