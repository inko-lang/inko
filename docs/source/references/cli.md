---
{
  "title": "Using the compiler"
}
---

Inko's compiler is available through the `inko` command.

## Compiling and running

To compile and then run a file, use the `inko run` command:

```bash
inko run hello.inko
```

This command is useful when running simple scripts or during the development of
your project. If your program defines any command line flags, specify them
_after_ the file to run:

```bash
inko run hello.inko --foo=bar
```

Any flags specified _before_ the file to run are treated as flags for the `run`
command.

## Compiling without running

The `inko run` command requires your source code to be available, and compiles
it from scratch every time. To avoid this, we can build a standalone executable
using the `inko build` command:

```bash
inko build hello.inko
```

The resulting executable is located at `./build/debug/hello`. By default no
optimizations are applied. To enable optimizations, use the `--release` flag:

```bash
inko build --release hello.inko
```

When using the `--release` flag, the executable is located at
`./build/release/hello`.

When compiling for the host/native target, build output is placed in `./build`
directly, but when building for a different architecture the output is scoped to
a directory named after that architecture. For example, when compiling for
arm64-linux-gnu on an amd64-linux-gnu host, build files are placed in
`./build/arm64-linux-gnu`.

For more information, run `inko --help`.
