//! Support for CPU prefetching.

/// Indicates if prefetching is available.
pub const ENABLED: bool = cfg!(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse"
));

/// Prefetches a pointer for a read operation.
///
/// On unsupported platforms this function will be a noop.
#[allow(unused_variables)]
pub fn prefetch_read<T>(pointer: *const T) {
    #[cfg(all(target_arch = "x86", target_feature = "sse"))]
    {
        use std::arch::x86 as arch_impl;

        unsafe {
            arch_impl::_mm_prefetch(
                pointer as *const i8,
                arch_impl::_MM_HINT_NTA,
            );
        }

        return;
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "sse"))]
    {
        use std::arch::x86_64 as arch_impl;

        unsafe {
            arch_impl::_mm_prefetch(
                pointer as *const i8,
                arch_impl::_MM_HINT_NTA,
            );
        }

        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefetch_read() {
        // This is just a smoke test to see if the function blows up or not.
        let thing = Box::new(10_u8);
        let ptr = &*thing as *const u8;

        prefetch_read(ptr);

        // Mostly to make sure the code doesn't just get optimised away.
        assert_eq!(unsafe { *ptr }, 10);
    }
}
