# Inko Compiler

**NOTE:** the compiler is still being developed and it is currently not really
usable.

This directory contains the source code of the Inko bytecode compiler, commonly
known as "inkoc". The compiler is currently written in Ruby but the long term
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

* Ruby 2.4 or newer
* Bundler
