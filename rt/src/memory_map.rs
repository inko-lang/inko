use crate::page::{multiple_of_page_size, page_size};
use libc::{
    c_int, mmap, mprotect, munmap, MAP_ANON, MAP_FAILED, MAP_PRIVATE,
    PROT_NONE, PROT_READ, PROT_WRITE,
};
use std::io::{Error, Result as IoResult};
use std::ptr::null_mut;

/// A chunk of memory created using `mmap` and similar functions.
pub(crate) struct MemoryMap {
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

fn mmap_options(_stack: bool) -> c_int {
    let base = MAP_PRIVATE | MAP_ANON;

    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd"
    ))]
    if _stack {
        return base | libc::MAP_STACK;
    }

    base
}

impl MemoryMap {
    pub(crate) fn new(size: usize, stack: bool) -> Self {
        let size = multiple_of_page_size(size);
        let opts = mmap_options(stack);

        let ptr = unsafe {
            mmap(null_mut(), size, PROT_READ | PROT_WRITE, opts, -1, 0)
        };

        if ptr == MAP_FAILED {
            panic!("mmap(2) failed: {}", Error::last_os_error());
        }

        MemoryMap { ptr: ptr as *mut u8, len: size }
    }

    pub(crate) fn protect(&mut self, start: usize) -> IoResult<()> {
        let res = unsafe {
            mprotect(self.ptr.add(start) as _, page_size(), PROT_NONE)
        };

        if res == 0 {
            Ok(())
        } else {
            Err(Error::last_os_error())
        }
    }
}

impl Drop for MemoryMap {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr as _, self.len);
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
