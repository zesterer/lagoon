use super::*;

pub struct TaskHandle<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> TaskHandle<T> {
    pub(crate) fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }

    pub fn join(self) -> Result<T, Error> {
        self.rx.recv().map_err(|_| Error::Timeout)
    }
}
