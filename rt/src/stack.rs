use crate::memory_map::MemoryMap;

pub(crate) fn total_stack_size(size: usize, page: usize) -> usize {
    // Round the user-provided size up to the nearest multiple of the page size.
    let rounded = (size + (page - 1)) & !(page - 1);

    // To allow masking stack pointers such that we get a pointer to the private
    // page, we need to ensure the size is a power of two.
    (page + page + rounded).next_power_of_two()
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
#[repr(C)]
pub struct Stack {
    mem: MemoryMap,
}

impl Stack {
    pub(crate) fn new(size: usize, page_size: usize) -> Self {
        let size = total_stack_size(size, page_size);
        let mut mem = MemoryMap::stack(size);

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
