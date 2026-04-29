use libc::{LOCK_EX, LOCK_UN, flock};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
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

        // We support Rust 1.85 or newer so we can't use `File::lock` as that
        // was first introduced in Rust 1.89.
        unsafe {
            let res = flock(file.as_raw_fd(), LOCK_EX);

            if res == -1 {
                return Err(io::Error::last_os_error());
            }
        }

        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
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
