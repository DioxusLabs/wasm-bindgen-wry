use std::time::Duration;

use criterion::Criterion;

mod batching;
mod roundtrip;

fn run_benchmarks(criterion: &mut Criterion) {
    let mut roundtrip_group = criterion.benchmark_group("roundtrip");
    roundtrip_group.bench_function("u32", |b| b.iter(roundtrip::bench_roundtrip_u32));
    roundtrip_group.bench_function("u64", |b| b.iter(roundtrip::bench_roundtrip_u64));
    roundtrip_group.bench_function("i32", |b| b.iter(roundtrip::bench_roundtrip_i32));
    roundtrip_group.bench_function("i64", |b| b.iter(roundtrip::bench_roundtrip_i64));
    roundtrip_group.bench_function("f32", |b| b.iter(roundtrip::bench_roundtrip_f32));
    roundtrip_group.bench_function("f64", |b| b.iter(roundtrip::bench_roundtrip_f64));
    roundtrip_group.bench_function("bool", |b| b.iter(roundtrip::bench_roundtrip_bool));
    roundtrip_group.bench_function("string", |b| b.iter(roundtrip::bench_roundtrip_string));
    roundtrip_group.bench_function("large-string", |b| {
        b.iter(roundtrip::bench_roundtrip_large_string)
    });
    roundtrip_group.bench_function("option_some", |b| {
        b.iter(roundtrip::bench_roundtrip_option_some)
    });
    roundtrip_group.bench_function("option_none", |b| {
        b.iter(roundtrip::bench_roundtrip_option_none)
    });
    roundtrip_group.finish();

    let mut batch = criterion.benchmark_group("batch");
    batch.bench_function("add_1_calls", |b| b.iter(batching::bench_batch_add_1));
    batch.bench_function("add_100_calls", |b| b.iter(batching::bench_batch_add_100));
    batch.bench_function("create_element_1_calls", |b| {
        b.iter(batching::bench_batch_create_element_1)
    });
    batch.bench_function("create_element_100_calls", |b| {
        b.iter(batching::bench_batch_create_element_100)
    });
    batch.finish();
}

fn criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(250))
        .measurement_time(Duration::from_millis(750))
        .sample_size(10)
        .configure_from_args()
        .without_plots()
}

fn main() {
    wry_launch::run_headless(|| async {
        let mut criterion = criterion();
        run_benchmarks(&mut criterion);
        criterion.final_summary();
    })
    .unwrap();
}
