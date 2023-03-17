use crate::page::{multiple_of_page_size, page_size};
use std::io::{Error, Result as IoResult};
use std::ptr::null_mut;

#[cfg(not(windows))]
use libc::{
    mmap, mprotect, munmap, MAP_ANON, MAP_FAILED, MAP_PRIVATE, PROT_NONE,
    PROT_READ, PROT_WRITE,
};

#[cfg(windows)]
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, VirtualProtect, MEM_COMMIT, MEM_RELEASE,
    MEM_RESERVE, PAGE_NOACCESS, PAGE_READWRITE,
};

#[cfg(any(target_os = "linux", target_os = "bsd"))]
fn mmap_options(stack: bool) -> std::os::raw::c_int {
    let opts = MAP_PRIVATE | MAP_ANON;

    if stack {
        opts | libc::MAP_STACK
    } else {
        opts
    }
}

#[cfg(not(any(target_os = "linux", target_os = "bsd")))]
fn mmap_options(_stack: bool) -> std::os::raw::c_int {
    MAP_PRIVATE | MAP_ANON
}

/// A chunk of memory created using `mmap` and similar functions.
pub(crate) struct MemoryMap {
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

impl MemoryMap {
    #[cfg(not(windows))]
    pub(crate) fn new(size: usize, stack: bool) -> Self {
        let size = multiple_of_page_size(size);
        let ptr = unsafe {
            mmap(
                null_mut(),
                size,
                PROT_READ | PROT_WRITE,
                mmap_options(stack),
                -1,
                0,
            )
        };

        if ptr == MAP_FAILED {
            panic!("mmap(2) failed: {}", Error::last_os_error());
        }

        Self { ptr: ptr as *mut u8, len: size }
    }

    #[cfg(windows)]
    pub(crate) fn new(size: usize, _stack: bool) -> Self {
        let size = multiple_of_page_size(size);
        let ptr = unsafe {
            VirtualAlloc(
                null_mut(),
                size,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
            )
        };

        if ptr.is_null() {
            panic!("VirtualAlloc() failed: {}", Error::last_os_error());
        }

        Stack { ptr: ptr as *mut u8, len: map_size }
    }

    #[cfg(not(windows))]
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

    #[cfg(windows)]
    pub(crate) fn protect(&mut self, start: usize) -> IoResult<()> {
        let res = unsafe {
            let mut old = 0;

            VirtualProtect(
                self.ptr.add(start),
                page_size(),
                PAGE_NOACCESS,
                &mut old,
            )
        };

        if res != 0 {
            Ok(())
        } else {
            Err(Error::last_os_error())
        }
    }
}

#[cfg(not(windows))]
impl Drop for MemoryMap {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr as _, self.len);
        }
    }
}

#[cfg(windows)]
impl Drop for MemoryMap {
    fn drop(&mut self) {
        unsafe {
            VirtualFree(self.ptr as _, self.len, MEM_RELEASE);
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
