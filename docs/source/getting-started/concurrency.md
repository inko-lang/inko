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
syntax is `uni T` (e.g. `uni Array[User]`). Mutable borrows are never sendable,
though in certain cases immutable borrows _are_ sendable (see below for more
details).

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

Calling methods on unique values is possible as long as the compiler is able to
guarantee this is safe. The basic requirement is that all arguments passed and
any values returned must be sendable for the method to be available. Since this
is overly strict in many instances, the compiler relaxes this rule whenever it
determines it's safe to do so. These exceptions are listed below.

### Immutable methods

If a method isn't able to mutate its receiver because it's defined as `fn`
instead of `fn mut`, it's safe to pass immutable borrows as arguments (which
aren't sendable by default):

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn friends_with?(user: ref User) -> Bool {
    @friends.contains?(user.name)
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let bob = User(name: 'Bob', friends: [])

    alice.friends_with?(bob)
  }
}
```

The reason this is safe is because `User.friends_with?` being immutable means
its `user` argument can't be stored in the `uni User` value stored in `alice`.
This _isn't_ possible if the method allows mutations (= it's an `fn mut` method)
because that would allow it to store the `ref User` in `self`.

### Mutable methods

There's an exception to the previous rule when it comes to the use of mutating
methods on unique receivers: if the compiler can guarantee the receiver can't
store any aliases to the returned data, it's in fact safe to use a mutating
method:

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn mut remove_last_friend -> Option[String] {
    @friends.pop
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let friend = alice.remove_last_friend
  }
}
```

Here the call to `User.remove_last_friend` _is_ allowed because the `User` type
doesn't store any borrows, nor do any sub values (e.g. the `Array` stored in the
`friends` field).

This is also true if the method is given any arguments, provided those arguments
are either sendable or immutable:

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn mut friends_with?(user: ref User) -> Bool {
    @friends.contains?(user.name)
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let bob = User(name: 'Bob', friends: ['Alice'])

    alice.friends_with?(bob) # => true
  }
}
```

The call `alice.friends_with?(bob)` is allowed because even though `ref User`
isn't sendable, the compiler knows that `alice` can't ever store a borrow and
thus it's safe to allow the call.

### Non-unique return values

If the return type of a method is owned and not unique (e.g. `Array[String]`
instead of `uni Array[String]`), the method _is_ available if it either doesn't
specify any arguments, all arguments are immutable borrows or all arguments
are sendable, _and_ the returned value doesn't contain any borrows:

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn friends -> Array[String] {
    @friends.clone
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])

    alice.friends
  }
}
```

Here the call `alice.friends` is valid because:

1. `User.friends` is immutable
1. `User.friends` doesn't accept any arguments
1. Because of this the `Array[String]` value can only be created from within
   `User.friends` and no aliases to it can exist upon returning it

The call isn't valid if the returned value contains borrows. For example:

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn borrow_self -> ref User {
    self
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let borrow = alice.borrow_self # => invalid
  }
}
```

In this case the call `alice.borrow_self` is rejected by the compiler because
it would result in an alias of the `uni User` value stored in `alice`. This is
also true if the borrow is a sub value:

```inko
type User {
  let @name: String
  let @friends: Array[String]

  fn borrow_self -> Option[ref User] {
    Option.Some(self)
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let borrow = alice.borrow_self # => invalid
  }
}
```

When the compiler verifies the return type to determine if it's sendable it also
verifies _all_ values stored within:

```inko
type Wrapper {
  let @user: ref User
}

type User {
  let @name: String
  let @friends: Array[String]

  fn wrap -> Wrapper {
    Wrapper(self)
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let wrapper = alice.wrap
  }
}
```

Here the call to `alice.wrap` is invalid because `Wrapper` defines a field of
type `ref User`, which isn't sendable.

### Unused return values

There's an exception to the above rule: if the returned value isn't sendable but
also isn't used by the caller, the method _can_ be used:

```inko
type User {
  let @name: String
  let mut @friends: Array[String]

  fn borrow_self -> ref User {
    self
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])

    alice.borrow_self
  }
}
```

Here `alice.borrow_self` _is_ allowed because its return value isn't used. This
also works when assigning the result to `_` using `let`:

```inko
type User {
  let @name: String
  let mut @friends: Array[String]

  fn borrow_self -> ref User {
    self
  }
}

type async Main {
  fn async main {
    let alice = recover User(name: 'Alice', friends: ['Bob'])
    let _ = alice.borrow_self
  }
}
```

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
- Return types aren't allowed, instead you must use a `Promise`, `Channel` or
  similar value to send the return value back to the sending (or another)
  process

## Dropping processes

Processes are value types, making it easy to share references to a process with
other processes. Internally processes use atomic reference counting to keep
track of the number of incoming references. When the count reaches zero, the
process is instructed to drop itself after it finishes running any remaining
messages. This means that there may be some time between when the last reference
to a process is dropped, and when the process itself is dropped.
