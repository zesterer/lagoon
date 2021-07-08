#[cfg(not(feature = "scope"))]
fn main() {}

#[cfg(feature = "scope")]
fn main() {
    let mut data = (0..100).collect::<Vec<u32>>();

    lagoon::ThreadPool::default().scoped(|s| {
        for x in data.iter_mut() {
            s.run(move || *x *= *x);
        }
    });

    assert!((0..100)
        .map(|x| x * x)
        .zip(data.into_iter())
        .all(|(x, y)| x == y));
}
