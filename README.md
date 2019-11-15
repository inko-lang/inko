# Inko

**Inko** is a gradually-typed, safe, object-oriented programming languages for
writing concurrent programs. By using lightweight isolated processes, data race
conditions can not occur. The syntax is easy to learn and remember, and thanks
to its error handling model you will never have to worry about unexpected
runtime errors.

For more information, see the [Inko website](https://inko-lang.org/).

## Features

* A bytecode interpreter that is easy to build across different platforms
* Parallel garbage collection based on [Immix][immix]
* Lightweight, isolated processes that communicate using message passing
* Gradual typing
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

* Ruby 2.3 or newer and RubyGems, for the compiler
* Rust 1.34 or newer, using the 2018 edition
* Make 4.0 or newer

## Installation

Installing all components can be done as follows:

    make install

You can uninstall all of them by running:

    make uninstall

By default everything is installed into directories relative to `/usr`. To
change this, pass the `PREFIX` variable to Make:

    make install PREFIX=$HOME/.local/share/inko

Individual components can be (un)installed by simply changing into the
appropriate directory (e.g. `cd compiler`), followed by running `make install`
or a similar command.

**NOTE:** The Makefile quotes the `PREFIX` variable in various places in order
to properly support spaces in file paths. This means that the tilde (`~`) in a
path is _not_ expanded. This means that commands like this won't work:

    make install PREFIX=~/.local/share/inko

Instead, use either the full path or use the `$HOME` variable:

    make install PREFIX=$HOME/.local/share/inko
    make install PREFIX=/home/alice/.local/share/inko

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".

[immix]: http://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf
[libffi]: https://sourceware.org/libffi/
