---
{
  "title": "Hello, concurrency!"
}
---

Let's make printing "Hello, world!" a little more exciting by performing work
concurrently. We'll start with creating the file `hello.inko` with the following
contents:

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

This program prints "Hello" and "world" concurrently to the terminal, then waits
500 milliseconds for this to complete.

To showcase this, run the program _several times_ as follows:

```bash
inko run hello.inko
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
  fn async print(message: String, output: uni Promise[Nil]) {
    let _ = Stdout.new.print(message)

    output.set(nil)
  }
}

type async Main {
  fn async main {
    let future1 = match Future.new {
      case (future, promise) -> {
        Printer().print('Hello', promise)
        future
      }
    }
    let future2 = match Future.new {
      case (future, promise) -> {
        Printer().print('world', promise)
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
  fn async print(message: String, output: uni Channel[Nil]) {
    let _ = Stdout.new.print(message)

    output.send(nil)
  }
}

type async Main {
  fn async main {
    let chan = Channel.new

    Printer().print('Hello', recover chan.clone)
    Printer().print('world', recover chan.clone)

    chan.receive
    chan.receive
  }
}
```
