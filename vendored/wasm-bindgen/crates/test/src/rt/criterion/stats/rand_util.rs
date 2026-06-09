use crate::__rt::web_time::{SystemTime, UNIX_EPOCH};
use core::cell::RefCell;
use oorandom::Rand64;
use wasm_bindgen::__rt::LazyCell;

pub type Rng = Rand64;

#[cfg_attr(target_feature = "atomics", thread_local)]
static SEED_RAND: LazyCell<RefCell<Rand64>> = LazyCell::new(|| {
    RefCell::new(Rand64::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis(),
    ))
});

pub fn new_rng() -> Rng {
    SEED_RAND.with(|seed_rand| {
        let mut r = seed_rand.borrow_mut();
        let seed = ((r.rand_u64() as u128) << 64) | (r.rand_u64() as u128);
        Rand64::new(seed)
    })
}
