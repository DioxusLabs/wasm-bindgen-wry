fn main() {
    lazy_js_bundle::LazyTypeScriptBindings::new()
        .with_watching("./src/ts")
        .with_binding("./src/ts/main.ts", "./src/js/main.js")
        .run();
}
