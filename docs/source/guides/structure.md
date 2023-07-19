# Project structure

Inko projects follow a simple structure: an `src/` directory containing your
modules, and a `test/` directory containing your unit tests. Source files use
the `.inko` extension, and should use lowercase names. For example, the source
code for `std::string` resides in `src/std/string.inko`.

Build files go in the `build/` directory. Inko creates this directory for
you if needed. This directory should not be tracked using your version control
system of choice.

Third-party dependencies are stored in a `dep/` directory. This directory is
managed using Inko's package manager, and you shouldn't put files in it
yourself.

## Libraries

If you are creating a library, its main module should be placed at
`src/NAME.inko` where `NAME` is the name of the library. For example, if you're
creating a library for interacting with [SQLite](https://sqlite.org/index.html),
you should place the main module at `src/sqlite.inko`. This way users can import
it using `import sqlite` when adding your library as a dependency.

## Executables

For executables, the main module must be placed at `src/main.inko`. When running
`inko build` without any arguments, the compiler tries to build this file if it
exists.

If you want to compile multiple executables, create a module for each executable
with an appropriate name (e.g. `src/foo.inko` and `src/bar.inko`), then specify
their paths when running `inko build` like so:

```bash
inko build src/foo.inko
inko build src/bar.inko
```

## Example layout

As a reference, this is what the standard library's structure looks like:

```
.
├── src
│   └── std
│       ├── array.inko
│       ├── bool.inko
│       ├── byte_array.inko
│       ├── clone.inko
│       ├── cmp.inko
│       ├── debug.inko
│       ├── drop.inko
│       ├── env.inko
│       ├── ffi.inko
│       ├── float.inko
│       ├── fmt.inko
│       ├── fs
│       │   ├── dir.inko
│       │   ├── file.inko
│       │   └── path.inko
│       ├── hash.inko
│       ├── ...
│       ├── time.inko
│       └── tuple.inko
└── test
    ├── helpers.inko
    ├── main.inko
    └── std
        ├── fs
        │   ├── test_dir.inko
        │   ├── test_file.inko
        │   └── test_path.inko
        ├── ...
        ├── test_array.inko
        ├── test_bool.inko
        ├── test_byte_array.inko
        ├── test_test.inko
        ├── test_time.inko
        └── test_tuple.inko
```
