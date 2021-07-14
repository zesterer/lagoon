use super::*;

use std::cell::RefCell;

/// A handle that refers to a job that notifies on completion. It may be created with [`ThreadPool::run_recv`].
pub struct JobHandle<T> {
    rx: oneshot::Receiver<T>,
    maybe_recv: RefCell<Option<T>>,
}

impl<T> JobHandle<T> {
    pub(crate) fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx, maybe_recv: RefCell::new(None) }
    }

    /// Returns whether the job associated with this handle has finished executing and can be joined without blocking.
    pub fn is_completed(&self) -> bool {
        match self.rx.try_recv() {
            Ok(x) => {
                // Stash the received value until joining later
                *self.maybe_recv.borrow_mut() = Some(x);
                true
            },
            Err(_) => false,
        }
    }

    /// Attempt to join the handle without blocking, returning an `Err` containing the handle if unsuccessful.
    pub fn try_join(self) -> Result<T, Self> {
        let x = self.maybe_recv.borrow_mut().take();
        if let Some(x) = x {
            Ok(x)
        } else {
            self.rx.try_recv().map_err(|_| self)
        }
    }

    /// Block the current thread, waiting for this job to complete.
    pub fn join(self) -> Result<T, Error> {
        if let Some(x) = self.maybe_recv.borrow_mut().take() {
            Ok(x)
        } else {
            self.rx.recv().map_err(|_| Error::Timeout)
        }
    }
}
