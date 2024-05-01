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
import std.stdio (STDOUT)
import std.time (Duration)

class async Printer {
  fn async print(message: String) {
    let _ = STDOUT.new.print(message)
  }
}

class async Main {
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
using the syntax `class async`, such as `class async Printer { ... }` in our
program.

We create instances of these processes using the syntax `Printer()`. For such a
process to do anything, we must send it a message. In our program we do this
using `print(...)`, where "print" is the message, defined using the `fn async`
syntax. The details of how this works, what to keep in mind, etc, are covered
separately.

The `sleep(...)` line is needed such that the main process (defined using
`class async Main`) doesn't stop before the `Printer` processes print the
messages to the terminal.

## Stopping right away when output is produced

Instead of waiting for a fixed 500 milliseconds, we can change the program to
stop right away when the output is produced. We achieve this by changing the
program to the following:

```inko
import std.stdio (STDOUT)

class async Printer {
  fn async print(message: String, channel: Channel[Nil]) {
    let _ = STDOUT.new.print(message)

    channel.send(nil)
  }
}

class async Main {
  fn async main {
    let channel = Channel.new(size: 2)

    Printer().print('Hello', channel)
    Printer().print('world', channel)
    channel.receive
    channel.receive
  }
}
```

What we changed here is that we're using the `Channel` type, and instead of
sleeping we wait for two messages to be received using `channel.receive`. The
`Printer` types are changed to send `nil` to the channel when they are finished.
The combination of the two results in the `Main` process waiting for both
`Printer` processes to write their output, then it stops.
