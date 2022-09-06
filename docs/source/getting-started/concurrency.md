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

!!! note
    The size of processes is subject to change, and we expect it to grow to 2
    KiB in the future (similar to Go and Erlang).

A process is defined using the syntax `class async`:

```inko
class async Counter {}
```

## The main process

Each program starts with a single process called "Main". This process runs on
the main thread, while other processes run on different threads. For this the
Inko runtime uses a pool of threads, balancing work between these threads
automatically.

The main process must be defined explicitly, and must define the async method
"main":

```inko
class async Main {
  fn async main {

  }
}
```

When the main process finishes and no references to it exist, the program is
ended; even if other processes are still running.

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
instead it will wait for its first message.

## Defining messages

Messages are defined by defining methods with the `async` keyword. A message is
just an asynchronous method call, optionally writing its result to a future. The
arguments, return type and throw type of an `async` method must be sendable (see
[Memory management](memory-management.md) for more information).

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

  fn async value -> Int {
    @value.clone
  }
}

class async Main {
  fn async main {
    let counter = Counter { @value = 0 }

    counter.increment
    counter.value # => 1
  }
}
```

When using this syntax, the sender is suspended until the receiver produces a
response. If we don't want to wait right away, we can do so using the `async`
keyword:

```inko
let counter = Counter { @value = 0 }

async counter.increment # => Future[Nil, Never]
async counter.value     # => Future[Int, Never]
```

When using the `async` keyword we get back a value of type `Future[T, E]`, where
`T` is the message's return type, and `E` the message's throw type (defaulting
to `Never`). To resolve the future you have to call `await` on it:

```inko
let counter = Counter { @value = 0 }

async counter.increment

let future = async counter.value

future.await # => 1
```

If the message may throw, the error has to be handled when calling `await`. This
isn't needed if the future's error type is `Never`, such as in the above
example.

`await` waits until a result is produced. If you want to limit the amount
of time spent waiting, use `Future.await_for` instead:

```inko
import std::time::Duration

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

    async counter.increment

    let future = async counter.value

    future.await_for(Duration.from_seconds(1))
  }
}
```

In this case the return type of `await_for` is `Option[Int]`, and a `None` is
produced if a result wasn't produced within the specified time limit.

## Polling futures

Sometimes you have multiple futures, and you want to act as soon as the
underlying message produces its result. For this there's the `poll` method from
the `std::process` module. This method takes an array of futures and waits until
one or more futures are ready. Its return value is an array of ready futures,
and the input array is modified in place so it no longer contains these futures:

```inko
import std::process::(poll)

class async Runner {
  fn async run {
    # Just imagine this doing something that may take a long time
    # ...
  }
}

class async Main {
  fn async main {
    let runner1 = Runner {}
    let runner2 = Runner {}
    let pending = [async runner1.run, async runner2.run]

    while pending.length > 0 {
      poll(pending).into_iter.each fn (future) {
        # ...
      }
    }
  }
}
```

The run time of `poll()` is `O(n)` where `n` is the number of futures to poll.
If you have a large list of futures to poll, it may be better to poll it in
smaller chunks, or refactor your code such that polling isn't necessary in the
first place.

## Dropping processes

The owned value for a process is a value type. Internally processes use atomic
reference counting to keep track of the number of incoming references. Imagine a
process being a server, and each owned value/reference being a client. When the
count reaches zero, the process is instructed to drop itself after it finishes
running any remaining messages. This means that there may be some time between
when the last reference to a process is dropped, and when the process itself is
dropped.
