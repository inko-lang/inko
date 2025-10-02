---
{
  "title": "Hello, concurrency!"
}
---

Let's make printing "Hello, world!" a little more exciting by performing work
concurrently.

To start, change `hello.inko` from the [](hello-world) tutorial to the
following:

```inko
import std.process (sleep)
import std.stdio (Stdout)
import std.time (Duration)

type async Printer {
  fn async print(message: String) {
    let _ = Stdout.new.print(message)
  }
}

type async Main {
  fn async main {
    Printer().print('Hello')
    Printer().print('world')
    sleep(Duration.from_millis(500))
  }
}
```

::: note
If you previously followed the [](sockets) tutorial, you should remove the
`src/client.inko` source file as we won't need it anymore.
:::

This program prints "Hello" and "world" concurrently to the terminal, then waits
500 milliseconds for this to complete.

To showcase this, run the program several times as follows:

```bash
inko build
./build/debug/hello
./build/debug/hello
./build/debug/hello
```

The output may change slightly between runs: sometimes it will print "Hello" and
"world" on separate lines, other times it may print "Helloworld", "worldHello"
or "world" and "Hello" on separate lines.

## Explanation

Inko uses "lightweight processes" for concurrency. Such processes are defined
using the syntax `type async`, such as `type async Printer { ... }` in our
program.

We create instances of these processes using the syntax `Printer()`. For such a
process to do anything, we must send it a message. In our program we do this
using `print(...)`, where "print" is the message, defined using the `fn async`
syntax. The details of how this works, what to keep in mind, etc, are covered
separately.

The `sleep(...)` line is needed such that the main process (defined using
`type async Main`) doesn't stop before the `Printer` processes print the
messages to the terminal.

## Futures and Promises

Instead of waiting for a fixed 500 milliseconds, we can change the program to
stop right away when the output is produced. We achieve this by changing the
program to the following:

```inko
import std.stdio (Stdout)
import std.sync (Future, Promise)

type async Printer {
  fn async print(output: uni Promise[Nil], message: String) {
    let _ = Stdout.new.print(message)

    output.set(nil)
  }
}

type async Main {
  fn async main {
    let future1 = match Future.new {
      case (future, promise) -> {
        Printer().print(promise, 'Hello')
        future
      }
    }
    let future2 = match Future.new {
      case (future, promise) -> {
        Printer().print(promise, 'world')
        future
      }
    }

    future1.get
    future2.get
  }
}
```

`Future` and `Promise` are types used for waiting for and resolving values
concurrently. A `Future` is a proxy for a value to be computed in the future,
while a `Promise` is used to assign a value to a `Future`.

In the above example, a `Promise` is passed along with the `Printer.print`
message, which is assigned to `nil`. The `Main` process in turn waits for this
to complete by calling `Future.get` on the two `Future` values. The result is
that the `Main` process doesn't stop until the `Printer` processes finished
their work.

The `Future` and `Promise` types are useful if a parent process wants to wait
for some result produced by a child process, without knowing what that parent
process is.

An example of this is a library function that wishes to perform computations in
parallel. The function doesn't know what processes it will be called from,
meaning any child processes can't communicate their results back to the parent
using messages. Using the `Future` and `Promise` types we _can_ achieve this.

## Async and await

Looking at the above code you might think to yourself "That looks rather
verbose, surely there's a better way?". Indeed there is! Inko has two types of
expressions to make working with `Future` and `Promise` values easier: `async`
expressions and `await` expressions.

An `async` expression is syntax sugar for calling `Future.new` and passing the
`Promise` to the called method, returning the `Future`:

```inko
# This expression:
async foo(10, 20)

# Is compiled into this:
match Future.new {
  case (future, promise) -> {
    foo(promise, 10, 20)
    future
  }
}
```

This means we can rewrite the `Printer` code to the following:

```inko
import std.stdio (Stdout)
import std.sync (Promise)

type async Printer {
  fn async print(output: uni Promise[Nil], message: String) {
    let _ = Stdout.new.print(message)

    output.set(nil)
  }
}

type async Main {
  fn async main {
    let future1 = async Printer().print('Hello')
    let future2 = async Printer().print('world')

    future1.get
    future2.get
  }
}
```

An `await` expression is similar to an `async` expression, instead it also
resolves the `Future` to its value using `Future.get`:

```inko
# This expression:
await foo(10, 20)

# Is compiled into this:
match Future.new {
  case (future, promise) -> {
    foo(promise, 10, 20)
    future.get
  }
}
```

This means we can further adjust the `Printer` example to the following:

```inko
import std.stdio (Stdout)
import std.sync (Promise)

type async Printer {
  fn async print(output: uni Promise[Nil], message: String) {
    let _ = Stdout.new.print(message)

    output.set(nil)
  }
}

type async Main {
  fn async main {
    await Printer().print('Hello')
    await Printer().print('world')
  }
}
```

::: note
Both `async` and `await` expressions only work with method call expressions
(e.g. `await [10, 20]` is invalid), and both require that the first argument of
the method is a `Promise`.
:::

The above example behaves a little different from the one that uses the `async`
expression: because `await` resolves the `Future`, the second call to `print`
doesn't run until the first one finishes, instead of the two calls running
concurrently.

As for when to use `async` versus `await`: if you need to compute something
asynchronously but can't continue without the result, use `await`. If you don't
need the result before continuing, use `async` instead and resolve the `Future`
to a value using methods such as `Future.get` and `Future.get_until` when you
need the result.

## Channels

Inko also provides a `Channel` type in the `std.sync` module, acting is an
unbounded multiple publisher, multiple subscriber channel. This type is useful
when M jobs need to be performed by N processes, where `M > N`. While sending
messages is certainly possible, it may result in an uneven workload across the
processes, but by using `std.sync.Channel` the workload is balanced
automatically.

We can rewrite the example from earlier using `Channel` as follows:

```inko
import std.stdio (Stdout)
import std.sync (Channel)

type async Printer {
  fn async print(output: uni Channel[Nil], message: String) {
    let _ = Stdout.new.print(message)

    output.send(nil)
  }
}

type async Main {
  fn async main {
    let chan = Channel.new

    Printer().print(recover chan.clone, 'Hello')
    Printer().print(recover chan.clone, 'world')

    chan.receive
    chan.receive
  }
}
```
