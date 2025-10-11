---
{
  "title": "Project structure"
}
---

Inko projects follow a simple structure: an `src/` directory containing your
modules, and a `test/` directory containing your unit tests. Source files use
the `.inko` extension, and should use lowercase names. For example, the source
code for [`std.string`]() resides in `src/std/string.inko`.

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

If you need to introduce additional modules, place them in a directory in `src`,
named after the library (e.g. `src/sqlite/statement.inko`).

## Executables

Executables are created by compiling files directly located in the `src/`
directory, with the executable file using the base name of the source file. For
example, `src/hello.inko` is compiled to an executable located at
`build/debug/hello` (or `build/release/hello` when using `inko build
--release`).

These source files must define the `async` type `Main` which in turn must define
the `async` method `main`:

```inko
type async Main {
  fn async main {
    # ...
  }
}
```

To build multiple executables, create multiple files in the `src/` directory.
For example, if your project contains the files `src/hello.inko` and
`src/world.inko` then running `inko build` produces two executables: `hello` and
`world`.

When building a library, don't define the `Main` type, and use `inko check`
instead of `inko build` to type-check your project.

## Example layout

Here's an example of a [typical Inko
project](https://github.com/yorickpeterse/kvi):

```
.
├── inko.pkg
├── LICENSE
├── Makefile
├── README.md
├── src
│   ├── kvi
│   │   ├── config.inko
│   │   ├── logger.inko
│   │   ├── map.inko
│   │   ├── num.inko
│   │   ├── resp.inko
│   │   └── server.inko
│   └── kvi.inko
└── test
    └── kvi
        ├── test_config.inko
        ├── test_logger.inko
        ├── test_map.inko
        ├── test_num.inko
        ├── test_resp.inko
        └── test_server.inko
```
