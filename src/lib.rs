#![feature(once_cell)]

#[cfg(feature = "scope")]
mod scope;
#[cfg(feature = "recv")]
mod recv;

#[cfg(feature = "scope")]
pub use scope::Scope;
#[cfg(feature = "recv")]
pub use recv::TaskHandle;

use std::{
    lazy::SyncLazy,
    thread::{self, JoinHandle},
    error,
    fmt,
    io,
};
// use flume::{Sender, unbounded};
use crossbeam_channel::{unbounded, Sender};

#[cfg(feature = "num_cpus")]
fn cpu_count() -> usize {
    num_cpus::get()
}

#[cfg(not(feature = "num_cpus"))]
fn cpu_count() -> usize {
    std::thread::available_concurrency()
        .map(|n| n.get())
        .unwrap_or(ThreadPool::DEFAULT_THREADS)
}

/// An error that may be produced when creating a [`ThreadPool`].
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The thread pool has no threads.
    NoThreads,
    /// A timeout occurred when attempting to join a task.
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

struct Task {
    f: Box<dyn FnOnce() + Send>,
}

static GLOBAL: SyncLazy<ThreadPool> = SyncLazy::new(|| ThreadPool::default());

/// A pool of threads that may be used to execute tasks.
pub struct ThreadPool {
    tx: Sender<Task>,
    handles: Vec<JoinHandle<()>>,
}

impl Default for ThreadPool {
    fn default() -> Self { Self::build().finish().unwrap() }
}

impl ThreadPool {
    /// The default number of threads that will be used if the available concurrency of the environment cannot be
    /// determined automatically.
    pub const DEFAULT_THREADS: usize = 8;

    /// Returns a reference to the global [`ThreadPool`].
    ///
    /// This should be used when you don't require any specific thread pool configuration to avoid multiple thread
    /// pools fighting for scheduler time. The global pool is created with the default configuration, i.e:
    /// [`ThreadPool::default`].
    pub fn global() -> &'static Self { &*GLOBAL }

    /// Begin building a new [`ThreadPool`].
    pub fn build() -> ThreadPoolBuilder {
        ThreadPoolBuilder::default()
    }

    /// Returns the number of threads in this pool.
    pub fn thread_count(&self) -> usize { self.handles.len() }

    /// Returns the number of tasks waiting to be executed.
    pub fn queue_len(&self) -> usize { self.tx.len() }

    /// Enqueue a function to be executed when a thread is free to do so.
    pub fn run<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.tx.send(Task { f: Box::new(f) }).unwrap()
    }

    /// Enqueue a function to be executed when a thread is free to do so, returning a handle that allows retrieval of
    /// the return value of the function.
    #[cfg(feature = "recv")]
    pub fn run_recv<F: FnOnce() -> R + Send + 'static, R: Send + 'static>(&self, f: F) -> recv::TaskHandle<R> {
        let (tx, rx) = oneshot::channel();
        self.run(move || { let _ = tx.send(f()); });
        recv::TaskHandle::new(rx)
    }

    /// Signal to threads that they should stop, then wait for them to finish processing tasks.
    ///
    /// All outstanding tasks will be executed before this function returns.
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
    /// This function will wait for all tasks created in the scope to finish before continuing.
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
    /// Configure the [`ThreadPool`] with the given number of threads.
    pub fn with_thread_count(self, thread_count: usize) -> Self {
        Self { thread_count: Some(thread_count), ..self }
    }

    /// Give the threads owned by this [`ThreadPool`] the given name.
    pub fn with_thread_name(self, name: String) -> Self {
        Self { thread_name: Some(name), ..self }
    }

    /// Give the threads owned by this [`ThreadPool`] a specific stack size.
    pub fn with_thread_stack_size(self, size: usize) -> Self {
        Self { thread_stack_size: Some(size), ..self }
    }

    /// Finish configuration, returning a [`ThreadPool`].
    pub fn finish(self) -> Result<ThreadPool, Error> {
        let thread_count = self.thread_count
            .unwrap_or(cpu_count());

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
                        while let Ok(task) = rx.recv() {
                            let task = std::panic::AssertUnwindSafe(task);
                            let _ = std::panic::catch_unwind(move || {
                                (task.0.f)();
                            });
                        }
                    }).map_err(Error::Io)
                })
                .collect::<Result<_, _>>()?,
        })
    }
}
