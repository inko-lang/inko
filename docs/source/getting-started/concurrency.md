---
{
  "title": "Concurrency and recovery"
}
---

The [](hello-concurrency) tutorial provides a basic overview of running code
concurrently. Let's take a look at the details of what makes concurrency safe in
Inko.

::: tip
This guide assumes you've read [](hello-concurrency) and [](memory-management),
as these guides explain the basics of what we'll build upon in this guide.
:::

To recap, Inko uses lightweight processes for concurrency. These processes don't
share memory, instead values are _moved_ between processes. Processes are
defined using `type async`:

```inko
type async Counter {
  let @number: Int
}
```

Interacting with processes is done using `async` methods. Such methods are
defined like so:

```inko
type async Counter {
  let mut @number: Int

  fn async mut increment(amount: Int) {
    @number += amount
  }
}
```

In the [](hello-concurrency) tutorial we only used value types as the arguments
for `async` methods, which are easy to move between processes as they're copied
upon moving. What if we want to move more complex values around?

Inko's approach to making this safe is to restrict moving data between processes
to values that are "sendable". A value is sendable if it's either a value type
(`String` or `Int` for example), or a unique value, of which the type signature
syntax is `uni T` (e.g. `uni Array[User]`).

## Unique values

::: note
If you're familiar with [Pony](https://www.ponylang.io/), Inko's unique values
are the same as Pony's isolated values, just using a name we feel better
captures their purpose/intent.
:::

A unique value is unique in the sense that only a single reference to it can
exist. The best way to explain this is to use a cardboard box as a metaphor: a
unique value is a box with items in it. Within that box these items are allowed
to refer to each other using borrows, but none of the items are allowed to refer
to values outside of the box or the other way around. This makes it safe to move
the data between processes, as no data race conditions can occur.

### Creating unique values

Unique values are created using the `recover` expression, and the return value
of such an expression is turned from a `T` into `uni T`, or from a `uni T` into
a `T`, depending on what the original type is:

```inko
let a = recover [10, 20] # => uni Array[Int]
let b = recover a        # => Array[Int]
```

This is why this process is known as "recovery": when the returned value is
owned we "recover" the ability to move it between processes. If the returned
value is instead a unique value, we recover the ability to perform more
operations on it (i.e. we lift the restrictions that come with a `uni T` value).

### Capturing variables

When capturing variables defined outside of the `recover` expression, they are
exposed using the following types:

|=
| Type on the outside
| Type on the inside
|-
| `T`
| `uni mut T`
|-
| `uni T`
| `uni T`
|-
| `mut T`
| `uni mut T`
|-
| `ref T`
| `uni ref T`

If a `recover` returns a captured `uni T` variable, the variable is _moved_ such
that the original one is no longer available.

### Borrowing unique values

Unique values can be borrowed using `ref` and `mut`, resulting in values of type
`uni ref T` and `uni mut T` respectively. These borrows come with signifiant
restrictions:

1. They can't be assigned to variables
1. They're not compatible with `ref T` and `mut T`, meaning you can't pass them
   as arguments.
1. They can't be used in type signatures

This effectively means they can only be used as method call receivers, provided
the method is available as discussed below.

## Unique values and method calls

Methods can be called on unique values provided the methods meet the following
criteria:

1. If a method takes any arguments and/or specifies a return type, these types
   must be sendable. If any of these types isn't sendable, the method isn't
   available.
1. If a method doesn't take any arguments and is immutable, and returns an owned
   value, the method is available if and only if these types are sendable
   (including any sub values they may store).

::: note
These restrictions can make working with unique values a bit tricky at times. We
aim to implement more sophisticated compiler analysis over time to make working
with unique values as easy as possible.
:::

To illustrate this, consider the following expression:

```inko
let a = recover 'testing'

a.to_upper
```

The variable `a` contains a value of type `uni String`. The expression
`a.to_upper` is valid because `to_upper` doesn't take any arguments, and its
return type (`String`) is a value type, which is a sendable type.

Because `a` is a unique value, we can also write the following:

```inko
let a = recover 'testing'  # => uni String
let b = recover a.to_upper # => uni String
```

Here's a more complicated example:

```inko
import std.net.ip (IpAddress)
import std.net.socket (TcpServer)

type async Main {
  fn async main {
    let server = recover TcpServer
      .new(IpAddress.v4(127, 0, 0, 1), port: 40_000)
      .get

    let client = recover server.accept.get
  }
}
```

Here `server` is of type `uni TcpServer`. The expression `server.accept` is
valid because `server` is unique and thus we can capture it, and because
`accept` meets rule two: it doesn't mutate its receiver, doesn't take any
arguments, and its return type is sendable.

Here's an example of something that isn't valid:

```inko
let a = recover [ByteArray.new]

a.push(ByteArray.new)
```

This isn't valid because `a` is of type `uni Array[ByteArray]`, and `push` takes
an argument of type `ByteArray` which isn't sendable, thus the `push` method
isn't available.

## Spawning processes with fields

When spawning a process, the values assigned to its fields must be sendable:

```inko
type async Example {
  let @numbers: Array[Int]
}

type async Main {
  fn async main {
    Example(numbers: recover [10, 20])
  }
}
```

## Defining async methods

When defining an `async` method, the following rules are enforced by the
compiler:

- The arguments must be sendable
- Return types aren't allowed

## Calling async methods

Calling `async` methods is done using the same syntax as for calling regular
methods:

```inko
import std.sync (Future, Promise)

type async Counter {
  let mut @value: Int

  fn async mut increment {
    @value += 1
  }

  fn async value(output: uni Promise[Int]) {
    output.set(@value)
  }
}

type async Main {
  fn async main {
    let counter = Counter(value: 0)

    counter.increment

    match Future.new {
      case (future, promise) -> {
        counter.value(promise)
        future.get # => 1
      }
    }
  }
}
```

## Dropping processes

Processes are value types, making it easy to share references to a process with
other processes. Internally processes use atomic reference counting to keep
track of the number of incoming references. When the count reaches zero, the
process is instructed to drop itself after it finishes running any remaining
messages. This means that there may be some time between when the last reference
to a process is dropped, and when the process itself is dropped.
