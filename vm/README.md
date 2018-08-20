# Inko Virtual Machine

This directory contains the source code of the Inko Virtual Machine, or "IVM"
for short.

IVM is a register based, garbage collected, object oriented bytecode virtual
machine that provides lightweight processes for concurrency. IVM is fairly
generic, providing a more general purpose object model and set of instructions
instead of features tailored specifically to Inko. This allows one to use IVM to
host other languages if desired, although this is not a primary feature of IVM.

IVM uses 3 pools of threads to perform its work:

1. A pool for garbage collecting processes
2. A pool for running lightweight processes
3. A secondary pool for running lightweight processes that may block the thread.
   This pool can be used for performing IO operations without blocking threads
   used for the primary pool.

The input of IVM is IVM Bytecode or "IBC" for short. IBC is a custom binary
format that is relatively easy to parse and fairly lightweight. IBC is portable
between architectures and operating systems, though it's best suited for 64 bits
platforms.

## Documentation

There is no proper documentation just yet for IVM, though various parts of the
source code are quite well documented. For now the source code is the best place
to start. Some files/directories that may be of interest are:

* `src/gc` and `src/immix`: the source code of the garbage collector and memory
  allocator.
* `src/bytecode_parser.rs`: the bytecode parser, useful if you're interested in
  the bytecode format.
* `src/object.rs`: the modules that defines how objects are layed out.
* `src/vm/machine.rs`: the source code of the instruction handlers.
* `src/vm/instruction.rs`: contains all available instructions.

## Requirements

* Rust 1.28 or newer
* Cargo
* Make

## Installation

Installing the VM is done as follows:

    make install

Uninstalling can be done as follows:

    make uninstall

By default the VM is installed in `/usr/bin/ivm`. The prefix (`/bin`) can be
changed by setting `PREFIX` when running `make install` or `make uninstall`:

    make install PREFIX=~/.local

## Building

To build the VM during development, simply run `make` in this directory. This
will generate a debug build. Other make tasks that are available:

* `make debug`: produces a debug build (the default)
* `make check`: runs `cargo check` to perform type verification
* `make test`: runs all the tests
* `make release`: produces a release build
* `make profile`: produces a release build without removing debugging symbols
* `make clean`: removes all build artifacts
* `make install`: builds and installs the VM
* `make uninstall`: uninstalls the VM
