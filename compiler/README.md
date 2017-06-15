# Inko Compiler

**NOTE:** the compiler is still being developed and it is currently not really
usable.

This directory contains the source code of the Inko bytecode compiler, commonly
known as "inkoc". The compiler is currently written in Rust but the long term
plan is to rewrite it in Inko and make it self hosting.

## Compilation Process

The compiler takes Inko source code, parses it into an AST, then converts it
into an IR called "Typed Intermediate Representation" or "TIR" for short. This
IR is a simplified version of the AST with type annotations added. This IR is
then used to verify the program, perform optimisations, and eventually generate
bytecode.

## Backends

The long term plan is to provide multiple backends for the compiler. In
particular the plan is to support both IVM bytecode, and either JavaScript or
WebAssembly; depending on how long it takes for WebAssembly to mature.

## Requirements

* Rust 1.10 or newer using a nightly build (stable Rust is not supported)
* Cargo
* Make

## Building

To build the compiler, simply run `make` in this directory. This will generate a
debug build. Other make tasks that are available:

* `make debug`: produces a debug build (the default)
* `make check`: runs `cargo check` to perform type verification
* `make test`: runs all the tests
* `make release`: produces a release build
* `make profile`: produces a release build without removing debugging symbols
* `make clean`: removes all build artifacts
