# Memory management

Inko uses automatic memory management, without the use of a garbage collector.
Instead, Inko relies on what is known as "single ownership". If you've used Rust
this should sound familiar, though Inko's implementation is quite different from
the implementation used by Rust.

The idea behind single ownership is as follows: each value has a single owner.
When this owner is done with the value, it discards it (known as a "drop").
Discarding the value runs its destructor (if it has any) and releases its
memory. Such values are referred to as "owned values".

A value is either owned by the scope it's created in, or another value it's
moved into. Take this code for example:

```inko
fn example {
  'hello'
}
```

The value `'hello'` is created in the outer-most scope of the `example` method,
meaning that said scope is the owner of the value. When we exit the scope (in
this case return from the `example` method), the value is dropped.

## Transferring ownership

Values can be _moved_, either into scopes or values. When a value is moved, its
owner is transferred to whatever it's moved into. For example, pushing a value
into an array makes the array the owner of the value:

```inko
fn example {
  let val = 'hello'
  let vals = [val]
}
```

When a value is moved, the previous owner no longer drops it, instead it's up to
the new owner to do so when necessary. A value that is moved can't be used
anymore, unless the value is a value type in which case a move copies the value.
Consider this example:

```inko
fn foo(values: Array[Int]) {
  # ...
}

fn bar {
  let vals = [10, 20]

  foo(vals)
}
```

The call to `foo` moves `vals`. If we try to use `vals` after the call, a
compile-time error is produced.

## References and borrowing

Besides owned values, Inko also supports references. A reference allows you to
temporarily use a value without moving it. When the reference is no longer
needed, only the reference is discarded; not the value it points to. Creating a
reference is known as "borrowing", because you borrow the value the reference
points to.

Inko supports two types of references: immutable (`ref T`) and mutable (`mut T`)
references. Immutable references, as the name suggests, don't allow you to
mutate the value pointed to, while mutable references do allow this. References
can only be created from an owned value, not from another reference.

References are created using the `ref` and `mut` keywords:

```inko
let a = [10, 20] # An owned value
let b = ref a    # An immutable reference
let c = mut a    # A mutable reference
```

Inko automatically creates references for you whenever necessary, so you won't
need to use these keywords often.

Unlike Rust, Inko allows you to move owned values when references to them exist.
Inko also allows both immutable and mutable references to the same value to
exist at the same time. This makes it trivial to implement self-referential data
structures (such as linked lists), without needing any unsafe code or the use of
raw pointers. For example, here's how you'd define a doubly-linked list:

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

To ensure correctness, Inko maintains a reference count at runtime. This count
tracks the amount of `ref` and `mut` references that exist for an owned value.
If the owned value is dropped but references to it still exist, a panic is
produced and the program is aborted; protecting you against use-after-free
errors.

The compiler tries to optimise code such that the amount of reference count
changes is kept as low as possible. While there is a runtime cost associated
with maintaining these reference counts, the cost is minimal, and far less than
the cost of running a tracing garbage collector or using regular (atomic)
reference counting everywhere.

Inko's implementation is inspired by the paper ["Ownership You Can Count
On"](https://www.semanticscholar.org/paper/Ownership-You-Can-Count-On/d0f2d28962d2a50d1914f0af8243d3f382fe077c)
([mirror](https://inko-lang.org/papers/ownership.pdf)).

## Unique values and recovery

Besides owned values and references, Inko also has "unique values" (`uni T`). A
unique value is unique in the sense that only a single reference to it can exist
(= the value itself). The best way to explain this is to use a cardboard box as
a metaphor: a unique value is a box with items in it. Within that box these
items are allowed to refer to each other using references, but none of the items
are allowed to refer to values outside of the box and vice-versa.

This restriction means that when we have a unique value we can move it around
knowing no references (outside of the unique value) exist that point to the
unique value. Inko's concurrency support builds upon this, and only allows you
to send values between processes if they are either unique or value types (which
are copied). This allows passing of data between processes, without you having
to worry about race conditions, and without a runtime cost such as having to
deep copy values.

If you're familiar with [Pony](https://www.ponylang.io/), Inko's unique values
are the same as Pony's "isolated values". This is not a coincidence, as both
Inko and Pony draw inspiration from the same paper ["Uniqueness and Reference
Immutability for Safe Parallelism"](https://www.microsoft.com/en-us/research/publication/uniqueness-and-reference-immutability-for-safe-parallelism/).
Unlike Pony, Inko doesn't have a long list of (complicated) reference
capabilities, making it easier to use Inko while still achieving the same
safety guarantees.

Unique values are created using the `recover` expression, and the return value
of such an expression is turned from a `T` into `uni T`, or from a `uni T` into
a `T`; depending on what the original type is:

```inko
let a = recover [10, 20] # => uni Array[Int]
let b = recover a        # => Array[Int]
```

Variables defined outside of the `recover` expression are exposed as `uni mut T`
or `uni ref T`, depending on what the original type is. Such values come with
the same restriction as `uni T` values, which are discussed in detail in the section
[Using unique values](#using-unique-values):

```inko
let nums = [10, 20, 30]

recover {
  nums # => uni mut Array[Int]
  [10]
}
```

Using `recover` we can statically guarantee it's safe to send values between
processes: if the only outside values we can refer to are unique values, then
any owned value returned must originate from inside the `recover` expression.
This in turn means any references created to it are either stored inside the
value (which is fine), or are discarded before the end of the `recover`
expression. That in turn means that after the `recover` expression returns, we
know for a fact no outside references to the unique value exist, nor can the
unique value contain any references to values stored outside of itself.

Values recovered using `recover` are moved, meaning that the old variable
containing the owned value is no longer available:

```inko
let a = recover [10, 20]
let b = recover a

a # => this is an error, because `a` is moved into `b`
```

In general, recovery is only needed when sending values between processes.

## Using unique values

Values of type `uni T`, `uni ref T` and `uni mut T` come with a variety of
restrictions to ensure their uniqueness constraints are maintained.

Values of type `uni ref T` and `uni mut T` can't be assigned to variables, nor
can you pass them to arguments that expect `ref T` or `mut T`. You also can't
use such types in type signatures. This means you can only use them as receivers
for method calls. As such, these kind of references don't violate the uniqueness
constraint of the `uni T` values they point to.

All three unique reference types allow you to call methods on values of such
types, provided the call meets the following criteria:

1. If a method takes any arguments and/or specifies a return type, these types
   must be "sendable". If any of these types isn't sendable, the method isn't
   available.
2. If a method doesn't take any arguments and is immutable, and returns an owned
   value, the method is available if and only if these types are sendable
   (including any sub values they may store).

A "sendable" type is a type that can cross the boundary between a unique value
and the outside world, or a type that can be sent to another process. A type is
sendable if it's unique (`uni T`), a value type, or an owned type that only
contains sendable types (in case of rule two). To put this another way: a
sendable type is a type of which we are certain no outside references to it
exist, and has no references pointing from it to the outside world.

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
import std::net::ip::IpAddress
import std::net::socket::TcpServer

class async Main {
  fn async main {
    let server = recover TcpServer
      .new(ip: IpAddress.v4(127, 0, 0, 1), port: 40_000)
      .unwrap

    let client = recover server.accept.unwrap
  }
}
```

Here `server` is of type `uni TcpServer`. The expression `server.accept` is
valid because `server` is unique and thus we can see it, and because `accept`
meets rule two: it doesn't mutate its receiver, doesn't take any arguments, and
the types it returns only store sendable types.

Here's an example of something that isn't valid:

```inko
let a = recover [ByteArray.new]

a.push(ByteArray.new)
```

This isn't valid because `a` is of type `uni Array[ByteArray]`, and `push` takes
an argument of type `ByteArray` which isn't sendable, thus the `push` method
isn't available.

## Panics and ownership

A panic is a critical error that aborts the program. When such an error is
produced, Inko doesn't drop any values and instead aborts right away.

## Benefits compared to garbage collection

If you're used to languages using (tracing) garbage collection you may wonder:
what's the benefit of all this?

The first benefit is that memory management using single ownership is
deterministic: every time you run your program, values are dropped in the same
place at the same time (assuming the drops are not influenced by some condition
of course). Not only does this make debugging easier, it also makes scaling your
program easier. This in particular is a problem for languages using a garbage
collector: they work fine for small workloads, but as load increases they tend
to need a lot of tuning, and even then they may not perform well enough. In some
cases one may even need to resort to hacks such as [allocating a 10 GiB byte
array](https://blog.twitch.tv/nl-nl/2019/04/10/go-memory-ballast-how-i-learnt-to-stop-worrying-and-love-the-heap/),
because the garbage collector doesn't provide the settings necessary to make it
perform better.

The second benefit is that because there's a clear point in the code where a
value is dropped, we can support deterministic destructors. This results in
simpler and more robust code. For example, external resources such as database
connections or files can be closed when a value goes out of scope, instead of
this requiring a manual call to a `close` or `dispose` method of sorts. While
some languages with a garbage collector support finalisers, finalisers are not
deterministic and may not run at all, and as such can't be relied upon for
anything important.

The third benefit is that single ownership can lead to better memory usage, or
at least more consistent memory usage. This isn't a hard guarantee as it depends
on the program's behaviour, but it's easier to achieve. For example, in typical
a garbage collected language the garbage collector doesn't kick in until a set
of conditions are met, such as the amount of memory allocated since the last
garbage collection run. This results in memory usage following a sawtooth
pattern, with memory usage increasing until the GC kicks in, at which point
memory usage _may_ be reduced (this is up to the GC implementation). When using
single ownership, memory is (at least typically) reclaimed as soon as possible,
which can lead to lower memory usage.

## The cost of single ownership

Single ownership isn't a silver bullet, and does come with a cost. Specifically,
the cost is that of having to run destructors and releasing values one by one,
instead of all at once. In case of Inko there's also a small cost involved in
maintaining reference counts. Should this cost become significant enough, the
solution is typically straightforward: adjust your code such that values live
longer (or shorter), or allocate values using an arena of sorts, and you're
probably good to go.

There's also a mental cost that comes with single ownership: as a developer
you're forced to decide who owns what value, when to transfer ownership, when to
use references, etc. In Rust this can be a challenge, as Rust is rather strict
about ownership. Inko tries to reduce this cost by being more forgiving and
shifting some compile-time work to work done at runtime. While this may not be
suitable for all types of programs, we believe it to be good enough for most of
them.
