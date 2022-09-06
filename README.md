# Inko

Inko is a general-purpose, statically typed, easy to use programming language.
It features deterministic automatic memory management (without a garbage
collector), a pleasant way of handling errors, excellent support for
concurrency, a simple syntax, and much more.

For more information, see the [Inko website](https://inko-lang.org/). If you'd
like to follow this project but don't have a GitLab account, please consider
starring our [GitHub mirror](https://github.com/YorickPeterse/inko).

## Features

- Deterministic automatic memory management based on single ownership
- Easy concurrency through lightweight isolated processes
- Static typing
- A unique twist on error handling
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

## Supported Platforms

[![CI sponsored by MacStadium](macstadium.png)](https://www.macstadium.com/)

Inko supports Linux, macOS and Windows. Other platforms such as FreeBSD or
OpenBSD may work, but are not officially supported at the moment.

## Installing

Details about how to install Inko and its requirements can be found in the
["Installing
Inko"](https://docs.inko-lang.org/manual/master/getting-started/installation/)
guide in the Inko manual.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".
