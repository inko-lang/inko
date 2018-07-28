# Inko Compiler

This directory contains the source code of the Inko bytecode compiler, commonly
known as "inkoc". The compiler is currently written in Ruby but the long term
plan is to rewrite it in Inko and make it self hosting.

## Usage

Compile a program:

    inkoc example.inko

Add a directory to the list of directories to use for source files:

    inkoc -i ../runtime/src example.inko

Use a custom directory for storing the bytecode files:

    inkoc -t /tmp/bytecode example.inko

If you want to use the compiler directly from the repository, you need to run
any of its executables as follows:

    env RUBYLIB=lib ./bin/inkoc --help

Without this, the executables will not be able to find the compiler's source
code.

## Requirements

* Ruby 2.3 or newer

## Installation

Manual installation from source is discouraged, as getting things to work this
way requires a bit more effort. Instead, it is recommended to use
[ienv](https://gitlab.com/inko-lang/ienv). If you truly want to install from the
repository, you can do so by running the following:

    make install

To run the executables you need to set the `RUBYLIB` environment variable to the
compiler's source directory. For example:

    make install PREFIX=example
    env RUBYLIB=example/lib/inko/compiler example/bin/inko --help

To uninstall:

    make uninstall PREFIX=example
