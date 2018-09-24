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

* Ruby 2.3 or newer and RubyGems, for the compiler
* Rust 1.28 or newer
* Make 4.0 or newer

## Supported Platforms

[![CI sponsored by MacStadium](macstadium.png)](https://www.macstadium.com/)

Inko supports any Unix-like platform, such as Linux, Mac OS, or BSD. Technically
Inko also works on Windows, but installing from source requires a Linux
compatibility layer such as [MSYS2](http://www.msys2.org/) or [Linux for
Windows](https://docs.microsoft.com/en-us/windows/wsl/install-win10).

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
