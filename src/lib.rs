//! Lagoon is a thread pool crate that aims to address many of the problems with existing thread pool crates.
//!
//! ## Features
//!
//! - **Scoped jobs**: Safely spawn jobs that have access to their parent scope!
//! - **Job handles**: Receive the result of a job when it finishes, or wait on it to finish!
//! - **Global pool**: A pay-for-what-you-use global thread pool that avoids dependencies fighting over resources!
//! - **Customise thread attributes**: Specify thread name, stack size, etc.
//!
//! ```ignore
//! let pool = lagoon::ThreadPool::default();
//!
//! // Spawn some jobs that notify us when they're finished
//! let jobs = (0..10)
//!     .map(|i| pool.run_recv(move || {
//!         println!("Hello! i = {}", i);
//!     }))
//!     .collect::<Vec<_>>();
//!
//! // Wait for all jobs to finish
//! for job in jobs {
//!     job.join().unwrap();
//! }
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]

#[cfg(feature = "scope")]
mod scope;
#[cfg(feature = "recv")]
mod recv;

#[cfg(feature = "scope")]
#[cfg_attr(docsrs, doc(cfg(feature = "scope")))]
pub use scope::Scope;
#[cfg(feature = "recv")]
#[cfg_attr(docsrs, doc(cfg(feature = "recv")))]
pub use recv::JobHandle;

use std::{
    thread::{self, JoinHandle},
    error,
    fmt,
    io,
};
// use flume::{Sender, unbounded};
use crossbeam_channel::{unbounded, Sender};

/// Attempt to determine the available concurrency of the host system.
///
/// In most cases, this corresponds to the number of CPU cores that are available to the program. If the `num_cpus`
/// feature is enabled (it is by default) the [`num_cpus`](https://crates.io/crates/num_cpus) crate will be used to
/// determine this value. Otherwise, the nightly-only `std::thread::available_concurrency` function will be used.
#[cfg(feature = "num_cpus")]
pub fn available_concurrency() -> Option<usize> {
    Some(num_cpus::get())
}

#[cfg(not(feature = "num_cpus"))]
pub fn available_concurrency() -> Option<usize> {
    std::thread::available_concurrency().map(|n| n.get())
}

/// An error that may be produced when creating a [`ThreadPool`].
#[derive(Debug)]
pub enum Error {
    /// An IO error occurred when attempting to spawn a thread.
    Io(io::Error),
    /// The thread pool has no threads.
    NoThreads,
    /// A timeout occurred when attempting to join a job.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{}", err),
            Self::NoThreads => write!(f, "thread pool has no threads"),
            Self::Timeout => write!(f, "a timeout occurred"),
        }
    }
}

impl error::Error for Error {}

struct Job {
    f: Box<dyn FnOnce() + Send>,
}

// TODO: Use when stable, see https://github.com/rust-lang/rust/issues/74465
//static GLOBAL: std::lazy::SyncLazy<ThreadPool> = std::lazy::SyncLazy::new(|| ThreadPool::default());

// I'll try spinning, that's a good trick! Actually, this isn't so bad: instead of spinning in a hot loop, we just
// yield to the scheduler every time we fail to access the global pool. This prevents priority inversion.
static GLOBAL: spin::once::Once<ThreadPool, spin::Yield> = spin::once::Once::new();

/// A pool of threads that may be used to execute jobs.
pub struct ThreadPool {
    tx: Sender<Job>,
    handles: Vec<JoinHandle<()>>,
}

impl Default for ThreadPool {
    fn default() -> Self { Self::build().finish().unwrap() }
}

impl ThreadPool {
    /// The default number of threads that will be used if the available concurrency of the environment cannot be
    /// determined automatically.
    pub const DEFAULT_THREAD_COUNT: usize = 8;

    /// Returns a reference to the global [`ThreadPool`], instantiating as with [`ThreadPool::default`] if it is not
    /// already initialized.
    ///
    /// This should be used when you don't require any specific thread pool configuration to avoid multiple thread
    /// pools fighting for scheduler time.
    pub fn global() -> &'static Self { Self::global_with_builder(ThreadPoolBuilder::default()) }

    /// Returns a reference to the global [`ThreadPool`], initializing it with the given [`ThreadPoolBuilder`] if it
    /// is not already initialized.
    ///
    /// Do *not* use this function if you are writing a library. Use [`ThreadPool::global`] instead so that the final
    /// binary crate is the one to determine the thread pool configuration (the end developer likely knows more about
    /// their specific threading requirements than you do and can use this information to optimise thread pool
    /// performance and minimise resource contention).
    ///
    /// If your application has specific pool requirements (for example, most games require thread pools to use N - 1
    /// threads to ensure that at least one core is free at any given time to keep the main thread running smoothly
    /// without stuttering) you should use this function *as early as possible* in the program's execution (i.e: at the
    /// top of the `main` function) to avoid dependencies initializing it first.
    ///
    /// Note additionally that the configuration you choose might interfere with dependencies that also use the global
    /// thread pool. Choose sensible, accomodating defaults where possible.
    pub fn global_with_builder(builder: ThreadPoolBuilder) -> &'static Self {
        GLOBAL.call_once(|| builder.finish().expect("Failed to initialise global thread pool"))
    }

    /// Begin building a new [`ThreadPool`].
    pub fn build() -> ThreadPoolBuilder {
        ThreadPoolBuilder::default()
    }

    /// Returns the number of threads in this pool.
    pub fn thread_count(&self) -> usize { self.handles.len() }

    /// Returns the number of jobs waiting to be executed.
    pub fn queue_len(&self) -> usize { self.tx.len() }

    /// Enqueue a function to be executed as a job when a thread is free to do so.
    ///
    /// ```
    /// let pool = lagoon::ThreadPool::default();
    ///
    /// for i in 0..10 {
    ///     pool.run(move || println!("I am the {}th job!", i));
    /// }
    /// ```
    pub fn run<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.tx.send(Job { f: Box::new(f) }).unwrap()
    }

    /// Enqueue a function to be executed as a job when a thread is free to do so, returning a handle that allows
    /// retrieval of the return value of the function.
    #[cfg(feature = "recv")]
    pub fn run_recv<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(&self, f: F) -> recv::JobHandle<R> {
        let (tx, rx) = oneshot::channel();
        self.run(move || { let _ = tx.send(f()); });
        recv::JobHandle::new(rx)
    }

    /// Signal to threads (not jobs) that they should stop, then wait for them to finish processing jobs.
    ///
    /// All outstanding jobs will be executed before this function returns.
    pub fn join_all(self) -> thread::Result<()> {
        let Self { tx, handles } = self;
        drop(tx);
        for handle in handles {
            handle.join()?;
        }
        Ok(())
    }

    /// Create a scope that allows the spawning of threads with safe access to the current scope.
    ///
    /// This function will wait for all jobs created in the scope to finish before continuing. See [`Scope`] for more
    /// information about scoped jobs.
    #[cfg(feature = "scope")]
    pub fn scoped<'pool, 'scope, F: FnOnce(scope::Scope<'pool, 'scope>) -> R, R>(&'pool self, f: F) -> R {
        scope::run(self, f)
    }
}

/// A type used to configure a [`ThreadPool`] prior to its creation.
#[derive(Clone, Default)]
pub struct ThreadPoolBuilder {
    thread_count: Option<usize>,
    thread_name: Option<String>,
    thread_stack_size: Option<usize>,
}

impl ThreadPoolBuilder {
    /// Configure the [`ThreadPool`] with the given number of threads. If unspecified, the thread pool will attempt to
    /// detect the number of hardware threads available to the process and use that. If this also fails,
    /// [`ThreadPool::DEFAULT_THREAD_COUNT`] will be used.
    pub fn with_thread_count(self, thread_count: usize) -> Self {
        Self { thread_count: Some(thread_count), ..self }
    }

    /// Give the threads owned by this [`ThreadPool`] the given name. If unspecified, the default name will be the same
    /// as those created by [`std::thread::spawn`].
    pub fn with_thread_name(self, name: String) -> Self {
        Self { thread_name: Some(name), ..self }
    }

    /// Give the threads owned by this [`ThreadPool`] a specific stack size. If unspecified, the default stack size
    /// will be the same as those created by [`std::thread::spawn`].
    pub fn with_thread_stack_size(self, size: usize) -> Self {
        Self { thread_stack_size: Some(size), ..self }
    }

    /// Finish configuration, returning a [`ThreadPool`].
    pub fn finish(self) -> Result<ThreadPool, Error> {
        let thread_count = self.thread_count
            .or_else(|| available_concurrency())
            .unwrap_or(ThreadPool::DEFAULT_THREAD_COUNT);

        if thread_count == 0 {
            return Err(Error::NoThreads);
        }

        let (tx, rx) = unbounded();

        Ok(ThreadPool {
            tx,
            handles: (0..thread_count)
                .map(|_| {
                    let rx = rx.clone();
                    let builder = thread::Builder::new();
                    let builder = match self.thread_name.clone() {
                        Some(name) => builder.name(name),
                        None => builder,
                    };
                    let builder = match self.thread_stack_size {
                        Some(size) => builder.stack_size(size),
                        None => builder,
                    };
                    builder.spawn(move || {
                        while let Ok(job) = rx.recv() {
                            let job = std::panic::AssertUnwindSafe(job);
                            let _ = std::panic::catch_unwind(move || {
                                (job.0.f)();
                            });
                        }
                    }).map_err(Error::Io)
                })
                .collect::<Result<_, _>>()?,
        })
    }
}
