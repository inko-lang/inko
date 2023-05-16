# Using the compiler

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

By default this produces a debug build, located in `./build/debug/hello`. To
produce a release build instead, run the following:

```bash
inko build --release hello.inko
```

The resulting executable is now located in `./build/release/hello`.

You can specify an alternative output path using the `-o` option:

```bash
inko build -o /tmp/hello hello.inko
```

For more information, run `inko --help`.
