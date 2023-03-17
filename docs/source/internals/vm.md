# The virtual machine

Inko runs your code using a custom register based bytecode interpreter, written
in Rust. The VM has over a hundred instructions, though many of these are
high-level instructions such as "send a message to a process" or "get the length
of a byte array".

For slower operations such as IO, the VM goes through an extra function call,
with each operation exposed as a function. These functions are called using the
instruction `BuiltinFunctionCall`. The decision to make something an instruction
or a built-in function is a bit arbitrary, but over time we'll probably turn
more instructions into built-in functions, meaning instructions are only used
for core operations such as comparing integers or getting the length of a
string.

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
this queue. Threads also have a single slot for a process to run at a higher
priority. This is used in certain cases where we want to reduce latency.

When new work is produced but the queue is full, the work is instead
pushed onto a global queue all threads have access to. Threads perform work in
these steps:

1. Run the process in the priority slot
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

Reductions are performed using the `Reduce` instruction, and it's up to the
compiler to insert these instructions in the right place.

## IO operations

### Sockets

For network IO the VM uses non-blocking sockets. When performing an operation
that would block, the process and its socket are registered with "the network
poller". This is a system/thread that polls a list of sockets until they are
ready, rescheduling their corresponding processes. Polling is done using APIs
such as epoll on Linux, kqueue on macOS/BSD, and IO completion ports on Windows.

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

The VM uses the system allocator for allocating memory. You can also build the
VM with jemalloc support as follows:

```bash
cargo build --release --features jemalloc
```

The use of jemalloc is recommended for production environments that don't
already use jemalloc as the system allocator.

In earlier versions of Inko we used a custom allocator based on
[Immix](https://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf).
We moved away from this for the following reasons:

- The implementation was quite complex and difficult to debug
- Immix suffers from fragmentation, and without a GC (what it's designed for)
  it's hard to clean up the fragmentation
- Our implementation was unlikely to outperform highly optimised allocators such
  as jemalloc, so we figured we may as well use an existing allocator and direct
  our attention elsewhere

## FFI

The VM provides an FFI using [libffi](https://sourceware.org/libffi/). By
default the VM builds libffi from source, but you can use a system wide
installation of libffi by building Inko as follows:

```bash
cargo build --release --features libffi-system
```

To use the FFI, the VM provides a few built-in functions such as
`ffi_library_open` and `ffi_function_attach`, called using the
`BuiltinFunctionCall` instruction.

## Bytecode format

The VM uses a custom bytecode format and is subject to change. The best way to
understand how it works is to look at the source code of the `Image` type in the
VM, found in `vm/src/image.rs`.

VM instructions need 12 bytes of space and support up to 5 arguments.

The VM provides the instruction `GetConstant` to read a constant (e.g. a string
literal) into a register. When the compiler generates these instructions, it
encodes a module-local constant index into the instruction. When the VM loads
the bytecode it rewrites these instructions to include a pointer to the
constant. This means that at runtime a `GetConstant` instruction just interprets
its arguments as a pointer and stores it in a register, instead of having to
deference several pointers and look up a value in an array using an index.

## Method dispatch

The VM supports two ways of calling regular methods: virtual dispatch and
dynamic dispatch. For virtual dispatch the VM reads the class of a receiver,
then looks up the method in the class' method table.

For dynamic dispatch we use hashing. The hash code for a method is based on its
name and generated at compile time by the bytecode compiler. The hash is
embedded into the instruction as an argument. To handle conflicts, the VM uses
linear probing. Due to rounding method table sizes up to the nearest power of
two (based on its raw/initial size), conflicts are quite rare and a method can
be found in at most a few probes. The VM doesn't perform inline caching, though
[we may implement this in the future](https://github.com/inko-lang/inko/issues/83).

Static methods are called using virtual dispatch on the class. We may add a
dedicated instruction for this in the future.

Method arguments are pushed onto a stack instead of using registers. This is
needed because certain operations may need more arguments than we can store in
an instruction. Arguments are pushed onto the stack using a `Push` instruction
and popped off the stack with a `Pop` instruction. When entering a method,
arguments are popped off the stack in reverse order, so the last argument is
popped first.

Each method call writes its result to a "result" field stored in a process. The
instruction `MoveResult` takes this value and stores it in a register, clearing
the "result" field in the process.

## Jump tables

The VM has a `JumpTable` instruction, primarily used for pattern matching
against enums. Jump tables are encoded in the bytecode image, and generated by
the compiler. The instruction is simple: it takes the value to test, which must
be an `Int`, and an index to the jump table (stored in the surrounding method).
The jump is then performed using essentially
`goto method.jump_tables[table_index][jump_value]`.

## Throwing

A throw is just a specialised return: the `Throw` instruction writes its value
to the "result" field, then sets the `thrown` flag to `true`. Error handling is
done using the `BranchResult` instruction, which checks the `thrown` flag and
branches accordingly. The compiler inserts this instruction after every method
call that may throw.

## Immediate values

The VM is able to optimise the allocations of small values, such as integers and
booleans. This is achieved using pointer tagging.

If a pointer has its lowest bit set to `1` it means the pointer is an immediate
value, instead of pointing to a heap allocated object.

If the lowest bit is set to `1` (so `xx1`), the pointer is a tagged integer
capable of storing integers up to 63 bits. 64 bits integers are heap allocated.

If the lowest two bits of a pointer are set to `10`, it means the pointer is a
pointer to a permanently allocated heap object, such as a string literal.

If the third lowest bit of a pointer is set to `1` (e.g. the pattern is `100`)
it means the pointer is a reference to an owned value.

Here's an overview of all these patterns (`x` indicates the bit could be set or
unset):

| Value            | Pattern
|:-----------------|:-----------
| Heap object      | `000`
| Permanent object | `x10`
| Reference        | `1x0`
| Tagged integer   | `xx1`

For more information, refer to `vm/src/mem.rs`, which implements most of the
memory management logic of the VM.

## Memory layout and fields

The VM uses the structure `Object` for regular class instance, and `Process` for
instances of processes. Both these Rust structures are variable-sized
structures, with the user-defined fields stored at the end. Each object has a
header that's 16 bytes. Instances of a class without any fields thus need only
16 bytes of memory, while an instance of a class with two fields needs 32 bytes
of memory (= each field is 8 bytes).

Different objects such as strings and arrays have a different layout, but all
objects _always_ start with a header.

When a class is defined, the number of fields is used to calculate the size of
instances of said class. When allocating such an instance we read the size from
the class and allocate accordingly.

Fields are just offsets to a structure, much like fields in e.g. C structures.
The VM doesn't use hashing for fields unlike e.g. Ruby and Python. Fields are
read using the `GetField` instruction, and written using the `SetField`
instruction. For processes the VM uses `ProcessGetField` and `ProcessSetField`
respectively, as processes have a slightly different memory layout.

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
