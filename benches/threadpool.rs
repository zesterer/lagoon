#[macro_use]
extern crate criterion;

use criterion::{Criterion, black_box};

const JOBC: usize = 100000;

fn lagoon_threadpool(threads: usize) {
    let pool = lagoon::ThreadPool::build()
        .with_thread_count(threads)
        .finish()
        .unwrap();
    for _ in 0..JOBC {
        pool.run(|| {
            let _ = black_box(8 + 9);
        });
    }
    pool.join_all().unwrap();
}

fn threadpool_threadpool(threads: usize) {
    let pool = threadpool::ThreadPool::new(threads);
    for _ in 0..JOBC {
        pool.execute(|| {
            let _ = black_box(8 + 9);
        });
    }
    pool.join();
}

fn uvth_threadpool(threads: usize) {
    let pool = uvth::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build();
    for _ in 0..JOBC {
        pool.execute(|| {
            let _ = black_box(8 + 9);
        });
    }
    pool.terminate();
}

fn rusty_pool_threadpool(threads: usize) {
    let pool = rusty_pool::ThreadPool::new(threads, threads, std::time::Duration::ZERO);
    for _ in 0..JOBC {
        pool.execute(|| {
            let _ = black_box(8 + 9);
        });
    }
    pool.shutdown_join();
}

fn criterion_benchmark(c: &mut Criterion) {
    let threads = num_cpus::get();
    let mut group = c.benchmark_group(format!("Spawning {} trivial tasks", JOBC));
    group.bench_function("lagoon_threadpool", |b| b.iter(|| lagoon_threadpool(threads)));
    group.bench_function("threadpool_threadpool", |b| b.iter(|| threadpool_threadpool(threads)));
    group.bench_function("uvth_threadpool", |b| b.iter(|| uvth_threadpool(threads)));
    group.bench_function("rusty_pool_threadpool", |b| b.iter(|| rusty_pool_threadpool(threads)));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
