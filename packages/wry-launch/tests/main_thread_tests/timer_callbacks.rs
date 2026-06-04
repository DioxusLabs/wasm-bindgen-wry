//! Regression test for JS-callback delivery: a `Closure` invoked from the JS
//! event loop must drive the app future so it observes the callback's effect.
//!
//! JS schedules N event-loop invocations of one Rust callback that increments a
//! counter; Rust waits until the counter reaches N. The failure it guards
//! against is a cross-thread race that only surfaces under contention, so run
//! many copies concurrently (e.g. 16x) to exercise it — a single run passes
//! whether or not the delivery path is correct.

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use wasm_bindgen::{Closure, wasm_bindgen};

pub(crate) async fn test_timer_callbacks() {
    #[wasm_bindgen(inline_js = "export function fire_many(cb, n) {
        for (let i = 0; i < n; i++) {
            setTimeout(() => { cb(); }, 0);
        }
    }")]
    extern "C" {
        fn fire_many(cb: &Closure<dyn FnMut()>, n: u32);
    }

    const N: u32 = 100;

    let count = Rc::new(Cell::new(0u32));
    let waker: Rc<RefCell<Option<Waker>>> = Rc::new(RefCell::new(None));

    let cb: Closure<dyn FnMut()> = {
        let count = count.clone();
        let waker = waker.clone();
        Closure::new(move || {
            count.set(count.get() + 1);
            if let Some(w) = waker.borrow_mut().take() {
                w.wake();
            }
        })
    };

    fire_many(&cb, N);

    poll_fn(|cx| {
        if count.get() >= N {
            Poll::Ready(())
        } else {
            *waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    })
    .await;

    assert_eq!(count.get(), N, "not all callbacks were delivered");
}
