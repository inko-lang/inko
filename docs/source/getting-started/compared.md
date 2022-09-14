# Inko compared to others

One may wonder what the differences are between Inko and their favourite
language. Below we try to give a subjective overview of these differences and
similarities. This isn't an exhaustive list, rather we focus on a few key
elements of the different languages.

## Erlang

!!! note
    While this section discusses Erlang specifically, most of this also applies
    to other languages running on the BEAM VM, such as Elixir and
    [Gleam](https://gleam.run/).

Erlang is a dynamically typed functional programming language that runs on a
custom bytecode interpreter, known as "BEAM". Erlang uses a tracing garbage
collector for managing memory. For concurrency it uses green threads. These
threads are isolated and can't share memory, and as such are referred to as
"lightweight processes".

Inko draws inspiration from Erlang, such as using similar terms (e.g.
"processes" for green threads) and using a similar approach for its preemptive
multitasking implementation. The most noteworthy differences are that Inko
doesn't rely on tracing garbage collection, and is statically typed. Using
single ownership instead of a garbage collector offers deterministic memory
management, and allows Inko to send values between processes without copying
them (unlike Erlang). The use of static typing means there's no need for pattern
matching against messages at runtime, as the compiler ensures you can only send
messages a receiver understands.

## Go

Go is statically typed, procedural, and compiles to machine code. For memory
management it uses tracing garbage collection. Concurrency is provided using
"goroutines", which are green threads scheduled by the Go runtime.

Goroutines share memory, either by using (mutable) global variables or by using
values sent using channels. Go's type system doesn't make any attempt at
preventing race conditions, instead requiring you to explicitly use
synchronisation APIs where necessary.

In contrast, Inko doesn't allow sharing of memory and its type system makes race
conditions impossible, removing the need for synchronisation. Combined with the
use of single ownership this leads to more reliable and easier to debug
software.

## Lunatic

[Lunatic](https://lunatic.solutions/) is a Web Assembly (WASM) runtime heavily
inspired by Erlang. Any language that compiles to WASM can run on Lunatic.

Since Lunatic is just a runtime and not a language, comparing it with Inko is a
bit tricky. For example, how memory is managed depends on the language compiled
to WASM, as Lunatic just implements the necessary APIs needed by WASM.

One difference is that Inko requires a 64-bits address space. While WASM can run
on 64-bits platforms, it only supports a 32-bits address space (64-bit address
spaces [are a work in progress](https://github.com/WebAssembly/memory64)). This
limits the maximum amount of memory a Lunatic program can use to 4 GiB.

## Ruby/Python

Ruby and Python are both dynamically typed, object-oriented, and both compile to
bytecode. Both Ruby and Python compile the bytecode when running your program,
though Python offers the option to compile bytecode ahead of time. Both use
tracing garbage collection, and both have a global interpreter lock (GIL) that
prevents running of multiple threads in parallel. Both use OS threads for
concurrency.

Ruby does allow running other OS threads when a thread is performing IO
operations, but it's not able to run threads in parallel when they perform CPU
bound work. Ruby 3.0 introduced "Ractors", which are essentially nested
interpreters. Each ractor can only run one OS thread at a time, but different
ractors can run in parallel.

Alternative implementations exist that allow for better concurrency, such as
[JRuby](https://www.jruby.org/), but these are far less commonly used than the
canonical implementations.

Inko runs processes using a fixed-size pool of OS threads. This means that for a
pool of N threads, N Inko processes can run in parallel. This number defaults to
the number of CPU cores but can be changed if needed. There's no global
interpreter lock, nor is there a need to use a different implementation just to
be able to run work in parallel.

Inko is also statically typed, and it requires errors to be handled at the call
site (instead of Python and Ruby which allow errors to surface anywhere). This
increases productivity, as you won't have to spend time chasing down unexpected
runtime errors.

A benefit of Python and Ruby is that both offer a vast amount of third-party
libraries, have plenty of jobs available, are financially supported by large
organisations, and have lots of resources available (books, tutorials, etc).
This makes both languages an excellent choice when performance isn't your number
one goal.

## Rust

Rust is a statically typed, object-oriented, and compiles to machine code. For
memory management both Rust and Inko use single ownership, though their
implementations, benefits and drawbacks differ. For example, Rust is strict at
compile-time, removing the need for runtime checks. Inko performs some of its
work at runtime, reducing the barrier to entry and making certain patterns
easier to implement.

Rust is an excellent language to use. In fact, Inko's virtual machine and
compiler are written in Rust. But Rust is undeniably a difficult language to
learn, and its safety guarantees (or more specifically how it enforces them) can
be difficult to work with. If you're looking for a more high level language that
offers similar safety guarantees and is easier to use, Inko might just be the
language for you.

## Pony

Pony is statically typed, object-oriented, and compiles to machine code. For
memory management it relies on tracing garbage collection ([see
here](https://tutorial.ponylang.io/appendices/garbage-collection.html)). For
concurrency it uses green threads (called "actors" in Pony) scheduled by the
Pony runtime. Pony uses cooperative multitasking, meaning it's possible for a
Pony actor to block an OS thread, such as by using an infinite loop. Pony actors
can share memory, though its type system imposes restrictions on what you can do
with shared memory.

Pony comes across as a rather complex language to use, mainly due to its many
reference capabilities. The website is also rather confusing to use, further
adding to the learning curve.

Inko shares similarities with Pony, as Inko's approach to concurrency is
inspired by Pony, which in turn is inspired by or based upon [this
paper](https://www.microsoft.com/en-us/research/publication/uniqueness-and-reference-immutability-for-safe-parallelism/).

Inko doesn't allow sharing of memory, instead it makes use of single ownership
and moves sent values into the receiver. This combined with Inko not having
nearly as many reference capabilities as Pony makes Inko easier to use.

Unlike Pony, Inko uses preemptive multitasking. This makes it impossible for an
Inko process to block an OS thread indefinitely.
