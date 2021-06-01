use super::*;

use std::{
    cell::Cell,
    marker::PhantomData,
    thread::{self, Thread},
    sync::{Arc, atomic::{AtomicUsize, Ordering}},
};

pub struct Scope<'pool, 'scope> {
    pool: &'pool ThreadPool,
    parent: Arc<(Thread, AtomicUsize)>,
    phantom: PhantomData<Cell<&'scope ()>>, // Use `Cell` for lifetime invariance
}

impl<'pool, 'scope> Scope<'pool, 'scope> {
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

    #[cfg(feature = "recv")]
    pub fn run_recv<F: FnOnce() -> R + Send + 'scope, R: Send + 'scope>(&self, f: F) -> recv::TaskHandle<R> {
        let (tx, rx) = oneshot::channel();
        self.run(move || { let _ = tx.send(f()); });
        recv::TaskHandle::new(rx)
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
