use lagoon::ThreadPool;

fn main() {
    let pool = ThreadPool::default();

    let tasks = (0..10)
        .map(|i| pool.run_recv(move || {
            println!("Hello! i = {}", i);
        }))
        .collect::<Vec<_>>();

    for task in tasks {
        task.join().unwrap();
    }
}
