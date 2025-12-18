use std::time::{Duration, Instant};
use wasm_bindgen::wasm_bindgen;
use wry_testing::{bindings::set_on_error, set_on_log};

mod batching;
mod roundtrip;

#[wasm_bindgen(inline_js = "export function heap_objects_alive(f) {
        return window.jsHeap.heapObjectsAlive();
    }")]
extern "C" {
    #[wasm_bindgen(js_name = heap_objects_alive)]
    pub fn heap_objects_alive() -> u32;
}

struct BenchResult {
    name: String,
    iterations: u64,
    total_time: Duration,
    avg_time: Duration,
}

impl BenchResult {
    fn print(&self) {
        println!(
            "{:<50} {:>12} iters  {:?}/iter  {:>10.2} ms total",
            self.name,
            self.iterations,
            self.avg_time,
            self.total_time.as_secs_f64() * 1000.0
        );
    }
}

fn bench<F: Fn()>(name: &str, f: F) -> BenchResult {
    let before_heap = heap_objects_alive();

    let warmup_iters = 10;
    for _ in 0..warmup_iters {
        f();
    }

    let iterations = 100u64;

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();

    let avg_time = elapsed.div_f64(iterations as f64);

    let after_heap = heap_objects_alive();
    if before_heap != after_heap {
        eprintln!(
            "WARNING: {} leaked {} heap objects",
            name,
            after_heap - before_heap
        );
    }

    BenchResult {
        name: name.to_string(),
        iterations,
        total_time: elapsed,
        avg_time,
    }
}

fn main() {
    wry_testing::run(|| {
        println!("\n{:=<100}", "");
        println!("{:^100}", "wry-bindgen Benchmarks");
        println!("{:=<100}\n", "");

        println!(
            "{:<50} {:>12}        {:>10}        {:>10}",
            "Benchmark", "Iterations", "Avg Time", "Total Time"
        );
        println!("{:-<100}", "");

        let mut results = Vec::new();

        results.push(bench("roundtrip/u32", roundtrip::bench_roundtrip_u32));
        results.push(bench("roundtrip/u64", roundtrip::bench_roundtrip_u64));
        results.push(bench("roundtrip/i32", roundtrip::bench_roundtrip_i32));
        results.push(bench("roundtrip/i64", roundtrip::bench_roundtrip_i64));
        results.push(bench("roundtrip/f32", roundtrip::bench_roundtrip_f32));
        results.push(bench("roundtrip/f64", roundtrip::bench_roundtrip_f64));
        results.push(bench("roundtrip/bool", roundtrip::bench_roundtrip_bool));
        results.push(bench("roundtrip/string", roundtrip::bench_roundtrip_string));
        results.push(bench("roundtrip/large-string", roundtrip::bench_roundtrip_large_string));
        results.push(bench(
            "roundtrip/option_some",
            roundtrip::bench_roundtrip_option_some,
        ));
        results.push(bench(
            "roundtrip/option_none",
            roundtrip::bench_roundtrip_option_none,
        ));

        results.push(bench("batch/add_1_calls", batching::bench_batch_add_1));
        results.push(bench("batch/add_100_calls", batching::bench_batch_add_100));
        results.push(bench("batch/create_element_1_calls", batching::bench_batch_create_element_1));
        results.push(bench("batch/create_element_100_calls", batching::bench_batch_create_element_100));

        for result in &results {
            result.print();
        }

        println!("{:=<100}\n", "");
    })
    .unwrap();
}
