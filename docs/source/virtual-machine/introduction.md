# Introduction

Inko is an interpreted language, and uses a register-based virtual machine. The
VM executes bytecode produced by the compiler, which is a separate program. The
VM uses preemptive multitasking for executing processes, and manages memory
using a garbage collector and allocator based on [Immix][immix].

The VM is written in [Rust](https://www.rust-lang.org/), and supports Linux,
macOS, and Windows. The VM only works on 64-bits platforms.

This sections covers various parts of the VM, such as its bytecode format, how
it manages memory, and more.

[immix]: http://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf
