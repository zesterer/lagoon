#![cfg_attr(docsrs, feature(doc_cfg))]

use super::*;

use std::{
    cell::Cell,
    marker::PhantomData,
    thread::{self, Thread},
    sync::{Arc, atomic::{AtomicUsize, Ordering}},
};

/// A scope within which jobs that refer to their parent scope may safely be spawned.
///
/// The example below demonstrates how scoped threads can easily be used to perform safe, ergonomic parallel data
/// processing, similar to [`rayon`](https://github.com/rayon-rs/rayon/).
///
/// *Note: This is a contrived example. The overhead of spawning jobs for each element of the vector will far outweigh
/// the cost of each operation in this case.*
///
/// ```
/// // A vector of the numbers 0 to 99
/// let mut data = (0..100).collect::<Vec<u32>>();
///
/// lagoon::ThreadPool::default().scoped(|s| {
///     // For each element in the vector...
///     for x in data.iter_mut() {
///         // ...spawn a job that squares that element
///         s.run(move || *x *= *x);
///     }
/// });
///
/// // Demonstrate that the elements have indeed been squared
/// assert!((0..100)
///     .map(|x| x * x)
///     .zip(data.into_iter())
///     .all(|(x, y)| x == y));
/// ```
pub struct Scope<'pool, 'scope> {
    pool: &'pool ThreadPool,
    parent: Arc<(Thread, AtomicUsize)>,
    phantom: PhantomData<Cell<&'scope ()>>, // Use `Cell` for lifetime invariance
}

impl<'pool, 'scope> Scope<'pool, 'scope> {
    /// Enqueue a function that may refer to its parent scope to be executed as a job when a thread is free to do so.
    pub fn run<F: FnOnce() + Send + 'scope>(&self, f: F) {
        let parent = self.parent.clone();
        parent.1.fetch_add(1, Ordering::Acquire);

        // Safety: we manually use `parent` to ensure that the calling scope lives long enough
        let f = unsafe { std::mem::transmute::<
            Box<dyn FnOnce() + Send + 'scope>,
            Box<dyn FnOnce() + Send + 'static>,
        >(Box::new(f)) };

        self.pool.run(move || {
            let _guard = scopeguard::guard(parent, |parent| {
                parent.1.fetch_sub(1, Ordering::Release);
                parent.0.unpark();
            });
            f();
        })
    }

    /// Enqueue a function that may refer to its parent scope to be executed as a job when a thread is free to do so,
    /// returning a handle that allows retrieval of the return value of the function.
    #[cfg(feature = "recv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "recv")))]
    pub fn run_recv<F: FnOnce() -> R + Send + 'scope, R: Send + 'scope>(&self, f: F) -> recv::JobHandle<R> {
        let (tx, rx) = oneshot::channel();
        self.run(move || { let _ = tx.send(f()); });
        recv::JobHandle::new(rx)
    }
}

pub(crate) fn run<'pool, 'scope, R>(pool: &'pool ThreadPool, f: impl FnOnce(Scope<'pool, 'scope>) -> R) -> R {
    let this = Arc::new((thread::current(), AtomicUsize::new(0)));

    let _guard = scopeguard::guard(this.clone(), |this| {
        while this.1.load(Ordering::SeqCst) > 0 {
            thread::park();
        }
    });

    f(Scope {
        pool,
        parent: this,
        phantom: PhantomData,
    })
}
