use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(windows)]
use windows_sys::Win32::System::SystemInformation::GetSystemInfo;

#[cfg(not(windows))]
use libc::{sysconf, _SC_PAGESIZE};

static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);

#[cfg(windows)]
fn page_size_raw() -> usize {
    unsafe {
        let mut info = MaybeUninit::uninit();

        GetSystemInfo(info.as_mut_ptr());
        info.assume_init_ref().dwPageSize as usize
    }
}

#[cfg(not(windows))]
fn page_size_raw() -> usize {
    unsafe { sysconf(_SC_PAGESIZE) as usize }
}

pub(crate) fn page_size() -> usize {
    match PAGE_SIZE.load(Ordering::Relaxed) {
        0 => {
            let size = page_size_raw();

            PAGE_SIZE.store(size, Ordering::Relaxed);
            size
        }
        n => n,
    }
}

pub(crate) fn multiple_of_page_size(size: usize) -> usize {
    let page = page_size();

    (size + (page - 1)) & !(page - 1)
}
