# Inko

Inko is a gradually-typed, safe, object-oriented programming languages for
writing concurrent programs. By using lightweight isolated processes, data race
conditions can not occur. The syntax is easy to learn and remember, and thanks
to its error handling model you will never have to worry about unexpected
runtime errors.

This repository contains the source code of the Inko compiler ("inkoc"), the
runtime, and the Inko virtual machine ("IVM").

For more information, see the [Inko website](https://inko-lang.org).

## Requirements

* Ruby 2.3 or newer and RubyGems, for the compiler.
* Rust nightly 1.28 or newer. Stable Rust is currently not supported.
* Make 4.0 or newer

## Installation

Installing all components can be done as follows:

    make install

You can uninstall all of them by running:

    make uninstall

By default everything is installed into directories relative to `/usr`. To
change this, pass the `PREFIX` variable to Make:

    make install PREFIX=~/.local

Individual components can be (un)installed by simply changing into the
appropriate directory (e.g. `cd compiler`), followed by running `make install`
or a similar command.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".
