use super::*;

/// A handle that refers to a job that notifies on completion. It may be created with [`ThreadPool::run_recv`].
pub struct JobHandle<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> JobHandle<T> {
    pub(crate) fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }

    /// Attempt to join the handle without blocking, returning an `Err` containing the handle if unsuccessful.
    pub fn try_join(self) -> Result<T, Self> {
        self.rx.try_recv().map_err(|_| self)
    }

    /// Block the current thread, waiting for this job to complete.
    pub fn join(self) -> Result<T, Error> {
        self.rx.recv().map_err(|_| Error::Timeout)
    }
}
