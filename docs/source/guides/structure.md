# Project structure

Inko projects follow a simple structure: an `src/` directory containing your
modules, and a `test/` directory containing your unit tests. Source files use
the `.inko` extension, and should use lowercase names. For example, the source
code for `std::string` resides in `src/std/string.inko`.

Build files should go in the `build/` directory. Inko creates this directory for
you if needed. This directory should not be tracked using your version control
system of choice.

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
