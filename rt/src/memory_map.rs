use crate::page::{multiple_of_page_size, page_size};
use rustix::mm::{
    mmap_anonymous, mprotect, munmap, MapFlags, MprotectFlags, ProtFlags,
};
use std::io::{Error, Result as IoResult};
use std::ptr::null_mut;

/// A chunk of memory created using `mmap` and similar functions.
pub(crate) struct MemoryMap {
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

fn mmap_options(_stack: bool) -> MapFlags {
    let base = MapFlags::PRIVATE;

    #[cfg(any(target_os = "linux", target_os = "freebsd",))]
    if _stack {
        return base | MapFlags::STACK;
    }

    base
}

impl MemoryMap {
    pub(crate) fn new(size: usize, stack: bool) -> Self {
        let size = multiple_of_page_size(size);
        let opts = mmap_options(stack);

        let res = unsafe {
            mmap_anonymous(
                null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                opts,
            )
        };

        match res {
            Ok(ptr) => MemoryMap { ptr: ptr as *mut u8, len: size },
            Err(e) => panic!(
                "mmap(2) failed: {}",
                Error::from_raw_os_error(e.raw_os_error())
            ),
        }
    }

    pub(crate) fn protect(&mut self, start: usize) -> IoResult<()> {
        let res = unsafe {
            mprotect(
                self.ptr.add(start) as _,
                page_size(),
                MprotectFlags::empty(),
            )
        };

        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::from_raw_os_error(e.raw_os_error())),
        }
    }
}

impl Drop for MemoryMap {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.ptr as _, self.len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let map1 = MemoryMap::new(32, false);
        let map2 = MemoryMap::new(page_size() * 3, false);

        assert_eq!(map1.len, page_size());
        assert_eq!(map2.len, page_size() * 3);
    }

    #[test]
    fn test_protect() {
        let mut map = MemoryMap::new(page_size() * 2, false);

        assert!(map.protect(0).is_ok());
    }
}
