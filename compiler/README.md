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

## Requirements

* Ruby 2.4 or newer
* Bundler

## Installation

For users:

    gem install inkoc

For developers:

    gem install bundler
    git clone https://gitlab.com/inko-lang/inko.git
    cd compiler
    bundle install

You can then use the compiler by running `./bin/inkoc`, or by installing it as a
Gem:

    rake install
