# Using Inko

Inko provides the command line executable `inko`. This executable is used for
both compiling and running Inko source code.

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

For deployments it's best to separate the two stages, as this removes the need
for deploying your source code. To compile (but not run) a file, run the
following:

```bash
inko build hello.inko
```

The resulting bytecode file is located at `./hello.ibi`, which you can run as
follows:

```bash
inko run hello.ibi
```

You can specify an alternative output path using the `-o` option:

```bash
inko build -o /tmp/hello.ibi hello.inko
```

For more information, run `inko --help`.
