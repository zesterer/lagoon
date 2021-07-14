#[cfg(not(feature = "recv"))]
fn main() { panic!("This example requires the `recv` feature") }

#[cfg(feature = "recv")]
fn main() {
    let pool = lagoon::ThreadPool::default();

    let jobs = (0..10)
        .map(|i| pool.run_recv(move || {
            println!("Hello! i = {}", i);
        }))
        .collect::<Vec<_>>();

    for job in jobs {
        job.join().unwrap();
    }
}
