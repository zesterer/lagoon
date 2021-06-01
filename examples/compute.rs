use lagoon::ThreadPool;

fn main() {
    let mut data = (0..100).collect::<Vec<u32>>();

    ThreadPool::default().scoped(|s| {
        for x in data.iter_mut() {
            s.run(move || *x *= *x).unwrap();
        }
    });

    assert!((0..100)
        .map(|x| x * x)
        .zip(data.into_iter())
        .all(|(x, y)| x == y));
}
