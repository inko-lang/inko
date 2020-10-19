# Inko

**Inko** is a statically-typed, safe, object-oriented programming languages for
writing concurrent programs. By using lightweight isolated processes, data race
conditions can't occur. The syntax is easy to learn and remember, and thanks to
its error handling model you will never have to worry about unexpected runtime
errors.

For more information, see the [Inko website](https://inko-lang.org/). If you'd
like to follow this project but don't have a GitLab account, please consider
starring our [GitHub mirror](https://github.com/YorickPeterse/inko).

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

Inko officially supports Linux, Mac OS, and Windows. Other Unix-like platforms
such as the various BSDs should also work, but are not officially supported at
this time. Inko only supports 64-bits architectures.

## Installing

Details about how to install Inko and its requirements can be found in the
["Installing
Inko"](https://docs.inko-lang.org/manual/master/getting-started/installation/)
guide in the Inko manual.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".

[immix]: http://www.cs.utexas.edu/users/speedway/DaCapo/papers/immix-pldi-2008.pdf
[libffi]: https://sourceware.org/libffi/
