use crate::memory_map::MemoryMap;
use rustix::param::page_size;
use std::collections::VecDeque;

/// The age of a reusable stack after which we deem it too old to keep around.
///
/// The value here is arbitrary, and chosen under the assumption it's good
/// enough to prevent excessive shrinking.
const SHRINK_AGE: u16 = 10;

/// The minimum number of stacks we need before we'll even consider shrinking
/// a stack pool.
///
/// The value here is arbitrary and mostly meant to avoid the shrinking overhead
/// for cases where it's pretty much just a waste of time.
const MIN_STACKS: usize = 4;

pub(crate) fn total_stack_size(size: usize, page: usize) -> usize {
    let total = page + page + size;

    // Rounds up to the nearest multiple of the page size.
    (total + (page - 1)) & !(page - 1)
}

/// A pool of `Stack` objects to reuse.
///
/// Stacks all share the same size and can't grow beyond this size. The decision
/// to not support growable stack is for the following reasons:
///
/// - It can mask/hide runaway recursion.
/// - It requires a prologue of sorts in every function to check if there's
///   enough stack space remaining.
/// - Depending on the backend used we may not have enough information to
///   accurately determine the amount of stack space a function needs.
/// - It's not clear when/if we would also shrink the stack, and doing this at
///   the right time is tricky.
/// - Foreign function calls would require that we either grow the stack to a
///   reasonable size, or swap the stack with a temporary stack of a larger
///   size. Both add complexity and incur a runtime cost we're not happy with.
/// - Even with a 1 MiB stack per process one can easily run millions of
///   processes and not run out of virtual memory.
/// - Processes aren't likely to use even close to the full amount of stack
///   space, so all the trouble of resizing stacks likely just isn't worth it.
/// - Fixed size stacks make it easier to reuse stacks, as we don't need to
///   divide them into size classes and pick the appropriate one based on the
///   size we need.
///
/// The amount of reusable stacks is reduced every now and then to prevent a
/// pool from using excessive amounts of memory.
pub(crate) struct StackPool {
    /// The size of memory pages.
    ///
    /// We store and reuse this value to avoid the system call overhead.
    page_size: usize,

    /// The amount of bytes to allocate for every stack, excluding guard pages.
    size: usize,

    /// Any stacks that can be reused.
    stacks: VecDeque<Stack>,

    /// The current epoch.
    ///
    /// This value is used to determine if reusable stacks have been sitting
    /// around for too long and should be discarded.
    epoch: u16,

    /// The check-in epoch for every stack.
    ///
    /// These values are stored separate from the stacks as we only need them
    /// when the stacks are reusable.
    epochs: VecDeque<u16>,
}

impl StackPool {
    pub fn new(size: usize) -> Self {
        Self {
            page_size: page_size(),
            size,
            stacks: VecDeque::new(),
            epoch: 0,
            epochs: VecDeque::new(),
        }
    }

    pub(crate) fn alloc(&mut self) -> Stack {
        if let Some(stack) = self.stacks.pop_back() {
            self.epoch = self.epoch.wrapping_add(1);
            self.epochs.pop_back();
            stack
        } else {
            Stack::new(self.size, self.page_size)
        }
    }

    pub(crate) fn add(&mut self, stack: Stack) {
        self.stacks.push_back(stack);
        self.epochs.push_back(self.epoch);
    }

    /// Shrinks the list of reusable stacks to at most half the current number
    /// of stacks.
    ///
    /// Using this method we can keep the number of unused stacks under control.
    /// For example, if we suddenly need many stacks but then never reuse most
    /// of them, this is a waste of memory.
    pub(crate) fn shrink(&mut self) {
        if self.stacks.len() < MIN_STACKS {
            return;
        }

        let trim_size = self
            .epochs
            .iter()
            .filter(|&&epoch| self.epoch.abs_diff(epoch) >= SHRINK_AGE)
            .count();

        // We want to shrink to at most half the size in an attempt to not
        // remove too many stacks we may need later.
        let max = std::cmp::min(self.stacks.len() / 2, trim_size);

        self.epochs.drain(0..max);
        self.stacks.drain(0..max);

        // Update the epochs of the remaining stacks so we don't shrink too soon
        // again.
        self.epochs = VecDeque::from(vec![self.epoch; self.epochs.len()]);
    }
}

/// A type that represents stack memory.
///
/// A `Stack` represents a chunk of memory of a certain size that can be used
/// as a process' stack memory. The exact implementation differs per platform.
///
/// The layout of the stack is as follows:
///
///     +--------------+
///     | private page |
///     +--------------+
///     |  guard page  |
///     +--------------+
///     |              | ^
///     |     stack    | | stack growth direction
///     |              |
///     +--------------+
///
/// The private page is used for storing data that generated code needs easy
/// access to, such as a pointer to the currently running process.
///
/// The entire memory region is aligned to its size, such that for any function
/// `stack pointer & -SIZE` produces an address that points to the start of the
/// private page.
#[repr(C)]
pub struct Stack {
    mem: MemoryMap,
}

impl Stack {
    pub(crate) fn new(size: usize, page_size: usize) -> Self {
        let size = total_stack_size(size, page_size);
        let mut mem = MemoryMap::new(size, true);

        // There's nothing we can do at runtime in response to the guard page
        // not being set up, so we just terminate if this ever happens.
        mem.protect(page_size, page_size).expect(
            "Failed to set up the stack's guard page. \
            You may need to increase the number of memory map areas allowed",
        );

        Self { mem }
    }

    pub(crate) fn private_data_pointer(&self) -> *mut u8 {
        self.mem.ptr
    }

    pub(crate) fn stack_pointer(&self) -> *mut u8 {
        unsafe { self.mem.ptr.add(self.mem.len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_pool_alloc() {
        let mut pool = StackPool::new(page_size());
        let size = page_size();
        let stack = pool.alloc();

        assert_eq!(stack.mem.len, size * 3);
    }

    #[test]
    fn test_stack_pool_alloc_with_reuse() {
        let size = page_size();
        let mut pool = StackPool::new(size);
        let stack = pool.alloc();

        pool.add(stack);
        assert_eq!(pool.stacks.len(), 1);
        assert_eq!(pool.epochs, vec![0]);

        pool.alloc();
        assert!(pool.stacks.is_empty());
        assert!(pool.epochs.is_empty());

        pool.add(Stack::new(size, page_size()));
        pool.add(Stack::new(size, page_size()));
        pool.alloc();
        pool.alloc();

        assert_eq!(pool.epoch, 3);
    }

    #[test]
    fn test_stack_pool_shrink() {
        let size = page_size();
        let mut pool = StackPool::new(size);

        pool.epoch = 14;

        pool.add(Stack::new(size, page_size()));
        pool.add(Stack::new(size, page_size()));
        pool.epochs[0] = 1;
        pool.epochs[1] = 2;

        // Not enough stacks, so no shrinking is performed.
        pool.shrink();

        pool.add(Stack::new(size, page_size()));
        pool.add(Stack::new(size, page_size()));
        pool.add(Stack::new(size, page_size()));
        pool.add(Stack::new(size, page_size()));

        pool.epochs[2] = 3;
        pool.epochs[3] = 4;
        pool.epochs[4] = 11;
        pool.epochs[5] = 12;

        // This shrinks the pool.
        pool.shrink();

        // This doesn't shrink the pool because we updated the epochs to prevent
        // excessive shrinking.
        pool.shrink();

        assert_eq!(pool.stacks.len(), 3);
        assert_eq!(&pool.epochs, &[14, 14, 14]);
    }
}
