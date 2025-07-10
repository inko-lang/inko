---
{
  "title": "The runtime"
}
---

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

## Multitasking

The scheduler uses cooperative multitasking, driven by the compiler. At various
points in the code, the compiler injects some extra code (called a "preemption
point") that checks if control should be yielded back to the scheduler.

Processes are given a time slice of 10 milliseconds, but may take a little
longer to run depending on their workload. The end goal is not to guarantee a
time slice of an exact amount of time, but rather to prevent a process from
never yielding back to the scheduler (i.e. when running an infinite loop).

Past versions used a fuel/reduction based approach similar to Erlang, but the
overhead was too great, see [this
issue](https://github.com/inko-lang/inko/issues/522) for more details.

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

## Stacks

Inko processes use fixed-size stacks created using `mmap()`. The default size is
1 MiB, but this can be changed at runtime. The size is always rounded up to the
nearest multiple of the page size. When allocating memory for a stack, the
runtime allocates some additional space for a guard page and additional private
data.

Stack memory is only committed as needed. This means that if a process only
needs 128 KiB of stack space, it only physically allocates 128 KiB; not the
entire 1 MiB. On Linux, the virtual address space limit is 128 TiB, enough for
134 217 728 Inko processes.

The layout of each stack is as follows:

```
╭───────────────────╮
│    Private page   │
├───────────────────┤
│     Guard page    │
├───────────────────┤
│                   │
│     Stack data    │ ↑ Stack grows towards the guard
│                   │
╰───────────────────╯
```

Stack values are allocated into the "Stack data" region. The guard page protects
against stack overflows. The private page contains data such as a pointer to the
process that owns the stack. The entire block is aligned to its size. This makes
it possible to get a pointer to the private page from the current stack pointer.

## Strings

Strings are immutable, and need at least 41 bytes of space. To allow easy
passing of strings to C, each string ends with a NULL byte on top of storing its
size. This NULL byte is ignored by Inko code. When passing a string to C, we
just pass the pointer to the string's bytes which includes the NULL byte.

Since C strings must be NULL terminated, the alternative would've been to create
a copy of the Inko string with a NULL byte at the end. When passing C strings to
Inko we'd then have to do the opposite, leading to a lot of redundant copying.
Our approach instead means we can pass strings between C and Inko with almost no
additional cost.

Strings use atomic reference counting when copying, meaning that a copy of a
string increments the reference count instead of creating a full copy.

## Signal handling

Signals are handled by a dedicated thread while other threads mask all signals
such that they don't receive any. This thread is known as the "signal handler".
The signal handler loops indefinitely, waiting for a signal to arrive at the
start of each iteration. When a signal is received, any processes waiting for
that signal are rescheduled. Waiting for a signal is performed using
`sigwait(3)`. If a signal is received for which no process is waiting, its
default behaviour is invoked, which in most cases means the process is
terminated.

When a process registers itself with the signal handler, the signal handling
thread may be waiting for a signal and needs to be woken up, such that it
observes the change in the list of processes waiting for a signal. This is
achieved by sending the SIGURG signal to the signal thread each time a process
waits for a signal to arrive. The signal SIGURG is used because of the following
reasons:

1. It's part of the POSIX 2001 specification, meaning it's well supported by
   POSIX compatible systems.
1. By default it's ignored, so our use won't conflict with what the system or
   user might expect by default.
1. Applications aren't likely to use it (if at all), in contrast to commonly
   used signals such as SIGUSR1 and SIGUSR2.

The standard library only allows waiting for a limited number of signals such as
SIGHUP, SIGUSR1, and SIGXFSZ. This is done to increase portability, and to make
it more difficult to shoot oneself in the foot (i.e. by waiting for SIGSEGV).
Waiting for SIGPIPE isn't supported as doing so may conflict with the use of
non-blocking sockets, and in general isn't useful to begin with.
