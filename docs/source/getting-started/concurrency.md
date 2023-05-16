# Concurrency

For concurrency Inko uses "lightweight processes", also known as green threads.
Processes are isolated from each other and don't share memory, making race
conditions impossible. Communication between processes is done by sending
messages, which look like regular method calls. Messages are processed in FIFO
order. Values passed with these messages have their ownership transferred to the
receiving process.

Processes run concurrently, and Inko's scheduler takes care of balancing the
workload across OS threads.

A process finishes when it has no more messages to process, and no references to
the process exist.

Processes are cheap to spawn, with a single empty process needing less than 1
KiB of memory.

A process is defined using the syntax `class async`:

```inko
class async Counter {}
```

## The main process

Each program starts with a single process called "Main". The main process must
be defined explicitly, and must define the async method "main":

```inko
class async Main {
  fn async main {

  }
}
```

When the main process finishes and no references to it exist, the program stops;
even if other processes are still running.

## Defining fields

A process can define zero or more fields:

```inko
class async Counter {
  let @value: Int
}
```

While the field types don't have to be sendable, when creating an instance the
assigned value _must_ be sendable. For example:

```inko
class async List {
  let @values: Array[Int]
}
```

To assign the `@values` field when creating an instance of `List`, we'd have to
assign it a unique value:

```inko
List { @values = recover [10, 20] }
```

Since a `uni Array[Int]` can be moved into a `Array[Int]`, this is valid. Had we
assigned it a regular array the program would not compile, because `Array[Int]`
isn't sendable.

## Spawning processes

A process is spawned by creating an instance of its class. In the above example,
`List { ... }` spawns the process for us, then gives us an owned value pointing
to the process. When spawning a process it doesn't start running right away,
instead it waits for its first message.

## Defining messages

Messages are defined by defining methods with the `async` keyword. The arguments
and return type of an `async` method must be sendable (see [Memory
management](memory-management.md) for more information).

`async` methods can't specify return types, and thus can't return any values.
Channels can be used when one process needs the result of an `async` method of
another process.

Here's how you'd define a message that just writes to STDOUT:

```inko
import std::stdio::STDOUT

class async Example {
  fn async write(message: String) {
    STDOUT.new.print(message)
  }
}
```

## Sending messages

Sending messages uses the same syntax as regular method calls:

```inko
class async Counter {
  let @value: Int

  fn async mut increment {
    @value += 1
  }

  fn async value(output: Channel[Int]) {
    output.send(@value)
  }
}

class async Main {
  fn async main {
    let counter = Counter { @value = 0 }
    let output = Channel.new(size: 1)

    counter.increment
    counter.value(output)
    output.receive # => 1
  }
}
```

Because `async` methods can't return a value, we must pass in a `Channel` to
send the output to and receive from.

## Dropping processes

Processes are value types, making it easy to share references to a process with
other processes. Internally processes use atomic reference counting to keep
track of the number of incoming references. When the count reaches zero, the
process is instructed to drop itself after it finishes running any remaining
messages. This means that there may be some time between when the last reference
to a process is dropped, and when the process itself is dropped.
