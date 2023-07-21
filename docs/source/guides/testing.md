# Unit testing

Inko's standard library provides a module used for writing unit tests:
`std.test`. Using this module one writes their unit tests like so:

```inko
import std.test.Tests

fn pub tests(t: mut Tests) {
  t.test('The name of the test') fn (t) {
    t.equal(10, 20)
  }
}
```

Each test can specify one or more expectations to check. If an expectation
fails, the failure is recorded and the test continues running. Not aborting the
test upon the first failure makes it easier to measure progress and makes the
testing process less frustrating, as you aren't shown only the first failure per
test.

Tests are run in a randomised order, and concurrently. By default the number of
concurrent tests equals the number of available CPU cores, but this can be
changed if necessary.

For more information about the testing API, take a look at the source code of
the `std.test` module.

!!! note
    In the future Inko will support generating source code documentation, at
    which point looking at the source code is no longer necessary.

## Testing structure

Test files follow the naming convention of `test_X.inko`, where X is the name of
the module tested. Tests are to be placed in a directory called `test` and
mirror the module hierarchy of the module they are testing. For example, the
tests for the standard library are organised as follows:

```
std/test/
├── main.inko
└── std
    ├── fs
    │   ├── test_dir.inko
    │   ├── test_file.inko
    │   └── test_path.inko
    ├── net
    │   ├── test_ip.inko
    │   └── test_socket.inko
    ├── test_array.inko
    ├── test_bool.inko
    ├── ...
    └── test_tuple.inko
```

In a test directory you should create a `main.inko` file. This file imports and
registers all your tests, and is run when using the `inko test` command. Here's
what such a file might look like:

```inko
import std.env
import std.test.(Filter, Tests)

import std.test_array
import std.test_bool
import std.test_byte_array
import std.test_tuple

class async Main {
  fn async main {
    let tests = Tests.new

    test_array.tests(tests)
    test_bool.tests(tests)
    test_byte_array.tests(tests)
    test_tuple.tests(tests)

    tests.filter = Filter.from_string(env.arguments.opt(0).unwrap_or(''))
    tests.run
  }
}
```

In the future Inko may generate this file for you, but for the time being it
needs to be maintained manually.

## Running tests

With these files in place you can run your tests using `inko test`. When doing
so, make sure your current working directory is the directory containing the
`test` directory, otherwise Inko won't find your unit tests.

The `inko test` command supports filtering tests by their name. For example, to
run tests of which the name contains "kittens" you'd run the tests like so:

```bash
inko test kittens
```

You can also filter by a file path, only running the tests in that file:

```bash
inko test test_kittens.inko
```

## Testing private types and methods

Following the structure outlined above, you're able to test private types and
methods, as tests and the modules they are testing both exist in the same root
namespace. That is, tests for `std.foo` would be located in the module
`std.test_foo`, and thus have access to private types and methods defined in any
module under the `std` root namespace. This in turn removes the need to mark
types or methods as public _just_ so you can test them.
