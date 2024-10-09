# <img src="https://inko-lang.org/images/logo.png?hash=4949e4795aafcdb1e6bbc31a555a9d4e82e65680656b8520831b1ced17c2a4d0" width="32" alt="Inko logo" /> Inko

Inko is a language for building concurrent software with confidence. Inko makes
it easy to build concurrent software, without having to worry about
unpredictable performance, unexpected runtime errors, or race conditions.

Inko features deterministic automatic memory management, move semantics, static
typing, type-safe concurrency, efficient error handling, and more. Inko source
code is compiled to machine code using [LLVM](https://llvm.org/).

For more information, refer to the [Inko website][website] or [the
documentation](https://docs.inko-lang.org). If you'd like to follow the
development of Inko, consider joining [our Discord
server](https://discord.gg/seeURxHxCb).

## Examples

Hello world:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    Stdout.new.print('Hello, world!')
  }
}
```

A simple concurrent program:

```inko
import std.sync (Future, Promise)

class async Calculator {
  fn async fact(size: Int, output: uni Promise[Int]) {
    let result = 1.to(size).iter.reduce(1, fn (product, val) { product * val })

    output.set(result)
  }
}

class async Main {
  fn async main {
    let calc = Calculator()

    match Future.new {
      case (future, promise) -> {
        # This calculates the factorial of 15 in the background, then we wait
        # for the result to be sent back to us.
        calc.fact(15, promise)
        future.get # => 1307674368000
      }
    }
  }
}
```

For more examples, refer to the [website][website].

## Installation

Details about how to install Inko and its requirements can be found in the
["Installing
Inko"](https://docs.inko-lang.org/manual/main/setup/installation/) guide in the
Inko manual.

## License

All source code in this repository is licensed under the Mozilla Public License
version 2.0, unless stated otherwise. A copy of this license can be found in the
file "LICENSE".

[website]: https://inko-lang.org/
