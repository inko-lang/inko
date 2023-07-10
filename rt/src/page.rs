use std::sync::atomic::{AtomicUsize, Ordering};

static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn page_size() -> usize {
    match PAGE_SIZE.load(Ordering::Relaxed) {
        0 => {
            let size = rustix::param::page_size();

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
