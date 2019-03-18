/// Joining of multiple threads.
use std::thread::{JoinHandle, Result as ThreadResult};

/// A JoinList can be used to join one or more threads easily.
pub struct JoinList<T> {
    handles: Vec<JoinHandle<T>>,
}

impl<T> JoinList<T> {
    /// Creates a new JoinList that will join the given threads.
    pub fn new(handles: Vec<JoinHandle<T>>) -> Self {
        JoinList { handles }
    }

    /// Waits for all the threads to finish.
    ///
    /// The return values of the threads are ignored.
    pub fn join(self) -> ThreadResult<()> {
        for handle in self.handles {
            handle.join()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_join() {
        let number = Arc::new(AtomicUsize::new(0));
        let number1 = number.clone();
        let number2 = number.clone();
        let handle1 =
            thread::spawn(move || number1.fetch_add(1, Ordering::SeqCst));

        let handle2 =
            thread::spawn(move || number2.fetch_add(1, Ordering::SeqCst));

        JoinList::new(vec![handle1, handle2]).join().unwrap();

        assert_eq!(number.load(Ordering::SeqCst), 2);
    }
}
