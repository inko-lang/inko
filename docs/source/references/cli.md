---
{
  "title": "Using the compiler"
}
---

Inko's compiler is available through the `inko` command. For a full list of
commands and options, run `inko --help`.

## Compiling executables

Building a standalone executable is done using the `inko build` command. Without
any arguments, this command compiles each source file directly located in the
`./src` directory to an executable. The name of the executable is derived from
the source file. For example, the file `./src/hello.inko` results in the
executable `./build/debug/hello`. You can also compile specific files, for
example:

```bash
inko build src/hello.inko
```

By default no optimizations are applied. To enable optimizations, use the
`--release` flag:

```bash
inko build --release
```

When using the `--release` flag, the executables are placed in the
`./build/release` directory.

When compiling for the host/native target, build output is placed in `./build`
directly, but when building for a different architecture the output is scoped to
a directory named after that architecture. For example, when compiling for
arm64-linux-gnu on an amd64-linux-gnu host, build files are placed in
`./build/arm64-linux-gnu`.

## Compiling and running

The `inko run` command is used for compiling and running a source file. Unlike
the `inko build` command, this command removes its build output (e.g. object
files) upon completion, meaning it has to compile the source file from scratch
every time. This makes it useful for running a script of sorts during
development:

```bash
inko run hello.inko
```

If your program accepts command line arguments, specify them _after_ the file to
run:

```bash
inko run hello.inko --foo=bar
```

Any arguments specified _before_ the file to run are treated as arguments for
the `run` command.
