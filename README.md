# Inko

**Inko** is a statically-typed, safe, object-oriented programming languages for
writing concurrent programs. By using lightweight isolated processes, data race
conditions can not occur. The syntax is easy to learn and remember, and thanks
to its error handling model you will never have to worry about unexpected
runtime errors.

For more information, see the [Inko website](https://inko-lang.org/).

## Features

* A bytecode interpreter that is easy to build across different platforms
* Parallel garbage collection based on [Immix][immix]
* Lightweight, isolated processes that communicate using message passing
* Statically typed
* Explicit handling of exceptions, making it impossible for unexpected
  exceptions to occur
* Tail call optimisation
* A C FFI using [libffi][libffi]
* A standard library written in Inko itself

More information about all the available features can be found [on the Inko
website](https://inko-lang.org/about/).

## Supported Platforms

[![CI sponsored by MacStadium](macstadium.png)](https://www.macstadium.com/)

Inko officially supports Linux, Mac OS, and Windows (when compiled with a MingW
toolchain such as [MSYS2](http://www.msys2.org/)). Other Unix-like platforms
such as the various BSDs should also work, but are not officially supported at
this time. Inko only supports 64-bits architectures.

## Requirements

Building from source requires the following software to be available:

* Ruby 2.3 or newer and RubyGems, for the compiler
* Rust 1.34 or newer, using the 2018 edition
* Make 4.0 or newer

For Unix systems or MSYS2 on Windows you also need the following software:

* autoconf
* automake
* libtool

### AES-NI

By default the VM is built with AES-NI support to speed up various hashing
operations. If your CPU does not support AES-NI, build the VM using either:

1. `cargo build --release` in the `vm/` directory
1. `make release RUSTFLAGS=""` in the `vm/` directory

Fortunately, pretty much all Intel and AMD x86-64 CPUs since 2010 have AES-NI
support, so disabling this is rarely necessary.

## Installation

Detailed installation instructions about the installation process can be found
at [Installing Inko](https://inko-lang.org/manual/install/) on the Inko website.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".

[immix]: http://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf
[libffi]: https://sourceware.org/libffi/
