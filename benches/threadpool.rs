#[macro_use]
extern crate criterion;

use criterion::Criterion;

const JOBC: usize = 100000;

fn lagoon_threadpool() {
    let pool = lagoon::ThreadPool::build()
        .with_thread_count(8)
        .finish()
        .unwrap();
    for _ in 0..JOBC {
        pool.run(|| {
            let _ = 8 + 9;
        });
    }
    pool.join_all().unwrap();
}

fn threadpool_threadpool() {
    let pool = threadpool::ThreadPool::new(8);
    for _ in 0..JOBC {
        pool.execute(|| {
            let _ = 8 + 9;
        });
    }
    pool.join();
}

fn uvth_threadpool() {
    let pool = uvth::ThreadPoolBuilder::new()
        .num_threads(8)
        .build();
    for _ in 0..JOBC {
        pool.execute(|| {
            let _ = 8 + 9;
        });
    }
    pool.terminate();
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("lagoon_threadpool", |b| b.iter(|| lagoon_threadpool()));
    c.bench_function("threadpool_threadpool", |b| b.iter(|| threadpool_threadpool()));
    c.bench_function("uvth_threadpool", |b| b.iter(|| uvth_threadpool()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
