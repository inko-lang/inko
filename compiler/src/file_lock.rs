use std::fs::File;
use std::io;
use std::path::Path;

pub struct FileLock {
    file: File,
}

impl FileLock {
    pub(crate) fn new(path: &Path) -> io::Result<Self> {
        let file = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)?;

        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::remove_file;
    use std::sync::mpsc::sync_channel;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_file_lock_new() {
        let (send, rec) = sync_channel(1);
        let path = temp_dir().join("inko-test-file-lock-new");

        thread::scope(|s| {
            let lock = FileLock::new(&path);
            let handle = s.spawn(|| {
                let _lock = FileLock::new(&path);
                let _ = send.send(true);
                true
            });

            // This will time out because we're holding on to the exclusive
            // lock.
            assert!(rec.recv_timeout(Duration::from_millis(25)).is_err());

            // Make sure the thread actually (eventually) acquires the lock and
            // then terminates.
            drop(lock);
            assert_eq!(handle.join().ok(), Some(true));
        });

        let _ = remove_file(&path);
    }
}
