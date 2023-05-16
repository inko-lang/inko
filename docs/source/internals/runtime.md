# The Inko runtime

Inko's native code compiler generates code to LLVM, linked against a small
runtime library written in Rust. The runtime library takes care of scheduling
processes on OS threads, polling sockets for readiness, etc.

## Running processes

Processes are scheduled onto a fixed-size pool of OS threads, with the default
size being equal to the number of CPU cores. This can be changed by setting the
environment variable `INKO_PROCESS_THREADS` to a value between 1 and 65 535.

### The main thread

The main OS thread isn't used for anything special, instead it waits for the
process threads to finish. This means that C libraries that require the use of
the main thread won't work with Inko. Few libraries have such requirements, most
of which are GUI libraries, and these probably won't work with Inko anyway due
to their heavy use of callbacks, which Inko doesn't support.

### Load balancing

Work is distributed using a work stealing algorithm. Each thread has a bounded
local queue that they produce work on, and other threads can steal work from
this queue.

When new work is produced but the queue is full, the work is instead
pushed onto a global queue all threads have access to. Threads perform work in
these steps:

1. Run all processes in the local queue
1. Steal processes from another thread
1. Steal processes from the global queue
1. Go to sleep until new work is pushed onto the global queue

### Reductions

Processes maintain a reduction counter, starting at a pre-determined value.
Certain operations reduce this counter. When the counter reaches zero it's
reset and the process is rescheduled. This ensures processes performing CPU
intensive work can't block OS threads indefinitely.

The default reduction count is 1000 and can be changed by setting the
environment variable `INKO_REDUCTIONS` to a value between 1 and 65 535. The
higher the value, the more time a process is allowed to run for.

## IO operations

### Sockets

For network IO the runtime uses non-blocking sockets. When performing an
operation that would block, the process and its socket are registered with "the
network poller". This is a system/thread that polls a list of sockets until they
are ready, rescheduling their corresponding processes. Polling is done using
APIs such as epoll on Linux, kqueue on macOS/BSD, and IO completion ports on
Windows.

By default a single network poller thread is spawned, and each process thread
uses the same poller. The number of poller threads is configured using the
`INKO_NETPOLL_THREADS` environment variable. This variable can be set to a value
between 1 and 127. When the value is greater than one, network poller threads
are assigned to process threads in a round-robin fashion. Most programs won't
need more than a single thread, but if you make heavy use of (many) sockets you
may want to increase this value.

### Blocking IO

For blocking operations, such as file IO, Inko uses a fixed amount of backup
threads. When an OS thread is about to enter a blocking operation, it sets a
flag indicating when it did so. This is implemented such that it in most cases
it won't take more than 100-200 nanoseconds.

In the background a monitor thread periodically examines all OS threads. If it
finds an OS thread is blocking for too long, it wakes up a backup thread to take
over the work of this blocking OS thread. When the blocking OS thread finishes
the blocking call it continues running its process. When the process is
rescheduled and the OS thread would pick up new work, it becomes a backup thread
instead.

The number of backup threads is controlled using the environment variable
`INKO_BACKUP_THREADS` and defaults to four times the number of CPU cores. The
monitor thread runs at an interval of 100 microseconds, though the exact
interval may differ between platforms. This interval can't be changed.

## Timeouts

Processes can suspend themselves with a timeout, or await a future for up to a
certain amount of time. A separate thread called the "timeout worker" handles
managing such processes. The timeout worker uses a binary heap for storing
processes along with their timeouts, sorting them such that those with the
shortest timeout are processed first.

When a process suspends itself with a timeout, it stores itself in a queue owned
by the timeout worker.

The timeout worker performs its work in these steps:

1. Move messages from the synchronised queue into an unsynchronised local FIFO
   queue
1. Defragment the heap by removing entries that are no longer valid (e.g. a
   process got rescheduled before its timeout expired)
1. Process any new entries to add into the heap
1. Sleep until the shortest timeout expires, taking into account time already
   spent sleeping for the given timeout
1. Repeat this cycle until we shut down

If the timeout worker is asleep and a new entry is added to the synchronised
queue, the worker is woken up and the cycle starts anew.

## Memory management

The runtime uses the system allocator for allocating memory. In earlier versions
of Inko we used a custom allocator based on
[Immix](https://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf).
We moved away from this for the following reasons:

- The implementation was quite complex and difficult to debug
- Immix suffers from fragmentation, and without a GC (what it's designed for)
  it's hard to clean up the fragmentation
- Our implementation was unlikely to outperform highly optimised allocators such
  as jemalloc, so we figured we may as well use an existing allocator and direct
  our attention elsewhere

## Strings

Strings are immutable, and need at least 41 bytes of space. To allow easy
passing of strings to C, each string ends with a NULL byte on top of storing its
length. This NULL byte is ignored by Inko code. When passing a string to C, we
just pass the pointer to the string's bytes which includes the NULL byte.

Since C strings must be NULL terminated, the alternative would've been to create
a copy of the Inko string with a NULL byte at the end. When passing C strings to
Inko we'd then have to do the opposite, leading to a lot of redundant copying.
Our approach instead means we can pass strings between C and Inko with almost no
additional cost.

Strings use atomic reference counting when copying, meaning that a copy of a
string increments the reference count instead of creating a full copy.
