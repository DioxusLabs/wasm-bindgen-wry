use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{Closure, wasm_bindgen};
use wry_launch::run;

#[wasm_bindgen(inline_js = r#"
    export function spam(cb) { setInterval(cb, 0); }
    export function nothing() {}
"#)]
extern "C" {
    fn spam(cb: &Closure<dyn Fn()>);
    fn nothing();
}

fn main() -> wry::Result<()> {
    run(|| async {
        let state = Rc::new(RefCell::new(0u64));

        let s = state.clone();
        let cb: Closure<dyn Fn()> = Closure::wrap(Box::new(move || *s.borrow_mut() += 1));
        spam(&cb);

        loop {
            let _held = state.borrow();
            // Hold a borrow across an js call. The callback fires reentrantly here.
            nothing();
        }
    })
}
