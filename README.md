# Inko

Inko is a language for building concurrent software with confidence. Inko makes
it easy to build concurrent software, without having to worry about
unpredictable performance, unexpected runtime errors, race conditions, and type
errors.

Inko features deterministic automatic memory management, move semantics, static
typing, type-safe concurrency, efficient error handling, and more.

Inko supports 64-bits Linux, macOS and Windows, and installing Inko is quick and
easy.

For more information, refer to the [Inko website](https://inko-lang.org/).

## Features

- Deterministic automatic memory management based on single ownership
- Easy concurrency through lightweight isolated processes
- Static typing
- Error handling done right
- An efficient, compact and portable bytecode interpreter
- Pattern matching
- Algebraic data types

## Examples

Here's what "Hello, World!" looks like in Inko:

```inko
import std::stdio::STDOUT

class async Main {
  fn async main {
    STDOUT.new.print('Hello, World!')
  }
}
```

And here's how you'd define a concurrent counter:

```inko
class async Counter {
  let @value: Int

  fn async mut increment {
    @value += 1
  }

  fn async value -> Int {
    @value.clone
  }
}

class async Main {
  fn async main {
    let counter = Counter { @value = 0 }

    # "Main" will wait for the results of these calls, without blocking any OS
    # threads.
    counter.increment
    counter.increment
    counter.value # => 2
  }
}
```

Inko uses single ownership like Rust, but unlike Rust our implementation is
easier and less frustrating to use. For example, defining self-referential data
types such as doubly linked lists is trivial, and doesn't require unsafe code or
raw pointers:

```inko
class Node[T] {
  let @next: Option[Node[T]]
  let @previous: Option[mut Node[T]]
  let @value: T
}

class List[T] {
  let @head: Option[Node[T]]
  let @tail: Option[mut Node[T]]
}
```

## Installing

Details about how to install Inko and its requirements can be found in the
["Installing
Inko"](https://docs.inko-lang.org/manual/master/getting-started/installation/)
guide in the Inko manual.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".
