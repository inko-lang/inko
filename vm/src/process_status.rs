//! Status types for processes.
use std::sync::atomic::{AtomicU8, Ordering};

/// The status of a process, represented as a set of bits.
///
/// We use an atomic U8 since an external process may read this value while we
/// are changing it (e.g. when a process sends a message while the receiver
/// enters the blocking status).
///
/// While concurrent reads are allowed, only the owning process should change
/// the status.
pub struct ProcessStatus {
    /// The bits used to indicate the status of the process.
    ///
    /// Multiple bits may be set in order to combine different statuses. For
    /// example, if the main process is blocking it will set both bits.
    bits: AtomicU8,
}

impl ProcessStatus {
    /// A regular process.
    const NORMAL: u8 = 0b0;

    /// The main process.
    const MAIN: u8 = 0b1;

    /// The process is performing a blocking operation.
    const BLOCKING: u8 = 0b10;

    /// The process is terminated.
    const TERMINATED: u8 = 0b100;

    pub fn new() -> Self {
        Self {
            bits: AtomicU8::new(Self::NORMAL),
        }
    }

    pub fn set_main(&mut self) {
        self.update_bits(Self::MAIN, true);
    }

    pub fn is_main(&self) -> bool {
        self.bit_is_set(Self::MAIN)
    }

    pub fn set_blocking(&mut self, enable: bool) {
        self.update_bits(Self::BLOCKING, enable);
    }

    pub fn is_blocking(&self) -> bool {
        self.bit_is_set(Self::BLOCKING)
    }

    pub fn set_terminated(&mut self) {
        self.update_bits(Self::TERMINATED, true);
    }

    pub fn is_terminated(&self) -> bool {
        self.bit_is_set(Self::TERMINATED)
    }

    fn update_bits(&mut self, mask: u8, enable: bool) {
        let bits = self.bits.load(Ordering::Acquire);
        let new_bits = if enable { bits | mask } else { bits & !mask };

        self.bits.store(new_bits, Ordering::Release);
    }

    fn bit_is_set(&self, bit: u8) -> bool {
        self.bits.load(Ordering::Acquire) & bit == bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_status() {
        let status = ProcessStatus::new();

        assert_eq!(status.is_main(), false);
        assert_eq!(status.is_blocking(), false);
        assert_eq!(status.is_terminated(), false);
    }

    #[test]
    fn test_set_main() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_main(), false);

        status.set_main();

        assert!(status.is_main());
    }

    #[test]
    fn test_set_blocking() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_blocking(), false);

        status.set_blocking(true);

        assert!(status.is_blocking());

        status.set_blocking(false);

        assert_eq!(status.is_blocking(), false);
    }

    #[test]
    fn test_set_terminated() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_terminated(), false);

        status.set_terminated();

        assert!(status.is_terminated());
    }
}
