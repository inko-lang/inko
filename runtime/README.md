# Inko Runtime

This directory contains the source code of the Inko Runtime, which consists out
of the core and standard library.

The core library (located in the `src/core/` directory) contains a few modules
critical for bootstrapping the runtime. The modules that make up the standard
library are located in the `src/std/` directory.

Unit tests for the standard library are located in the `tests/` directory.

## Installation

Installing the standard library is done as follows:

    make install

Uninstalling can be done as follows:

    make uninstall

By default the standard library is installed in `/usr/lib/inko/VERSION`, with
`VERSION` being the language version (as specified in the `LANGUAGE_VERSION`
file). The prefix (`/usr`) can be changed by setting `PREFIX` when running `make
install` or `make uninstall`:

    make install PREFIX=~/.local
