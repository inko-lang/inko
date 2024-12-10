---
{
  "title": "Memory management"
}
---

Inko uses automatic memory management, using "single ownership" instead of a
garbage collector. Inko has four primary types of references: owned references,
immutable borrows, mutable borrows, and unique references. In this tutorial
we'll take a look at the basics of working with these different references.

## Owned references

We'll start with a simple list of cats:

```inko
class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
  }
}
```

Save this in a file called `cats.inko` and run it as follows:

```bash
inko run cats.inko
```

If all went well, no output is produced.

Let's adjust the program to print the number of cats to the terminal:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]

    Stdout.new.print('${cats.size} cats')
  }
}
```

If we run this again, the output is "2 cats".

Now we'll change the program to the following:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let more_cats = cats

    Stdout.new.print('${cats.size} cats')
  }
}
```

If we try to run this program, we're greeted with the following compile-time
error:

```
cats.inko:10:24 error(moved): 'cats' can't be used as it has been moved
```

What happened is as follows: `cats` is a variable containing an owned reference
to the array of cats. When this reference is assigned to the `more_cats`
variable, ownership of the reference is _moved_ to the `more_cats` variable.
Once ownership is moved, we can no longer use the old reference (`cats` in this
case).

When the owned reference is no longer in use, the type's destructor is run (if
it defines any) and its memory is released. This process is known as "dropping"
a value.

## Borrowing

If we only had owned references, writing meaningful programs in Inko would be
difficult. Inko has two types of references that don't transfer ownership, known
as "borrows": immutable borrows, and mutable borrows.

Immutable borrows are created using the `ref` keyword:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let more_cats = ref cats

    Stdout.new.print('${cats.size} cats')
  }
}
```

Mutable borrows are created using the `mut` keyword:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let more_cats = mut cats

    Stdout.new.print('${cats.size} cats')
  }
}
```

Running both these programs produces the output "2 cats", without any
compile-time errors.

### Mutable vs immutable

The difference between immutable and mutable borrows is simple: mutable borrows
allow mutating of the borrowed data, while immutable borrows don't. For example:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let cats_ref = ref cats

    cats_ref.pop
  }
}
```

If you try to run this program, you'll be greeted with the following
compile-time error:

```
cats.inko:10:5 error(invalid-call): the method 'pop' requires a mutable receiver, but 'ref Array[Cat]' isn't mutable
```

To fix this, we need to use a mutable borrow:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let cats_mut = mut cats

    cats_mut.pop
  }
}
```

### Automatic borrows

When passing a value to something that expects a borrow, Inko automatically
borrows the value according to the expected borrow:

```inko
class Person {
  let @name: String
}

fn example(person: ref Person) {}

class async Main {
  fn async main {
    let person = Person(name: 'Alice')

    example(person)
  }
}
```

Here `example(person)` results in the compiler passing a `ref Person` as the
argument, allowing you to continue using `person` after returning from the
`example` call. The behavior of automatically borrowing values is as follows:

|=
| Input
| Expected
| Passed
|-
| `T`
| `ref T`
| `ref T`
|-
| `T`
| `mut T`
| `mut T`
|-
| `ref T`
| `ref T`
| `ref T`
|-
| `mut T`
| `mut T`
| `mut T`
|-
| `mut T`
| `ref T`
| `ref T`

### Moving while borrowing

Inko allows you to move owned references while borrows to the owned reference
exist. If an owned value is dropped while borrows to it still exist, a runtime
error known as a "panic" is produced, terminating the program:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = [Cat(), Cat()]
    let borrow = ref cats
    let more_cats = cats

    Stdout.new.print('${borrow.size} cats')
    Stdout.new.print('${more_cats.size} cats')
  }
}
```

If we run this program, the output is as follows:

```
2 cats
2 cats
Stack trace (the most recent call comes last):
  [...]/cats.inko:12 in main.Main.main
  [...]/std/src/std/array.inko:104 in std.array.Array.$dropper
Process 'Main' (0x55fd6eb37170) panicked: can't drop a value of type 'Array' as it still has 1 reference(s)
```

The reason this happens is because `more_cats` is dropped before `borrow` is
dropped, while the borrow still exists (because `borrow` is defined before
`more_cats`), resulting in this error.

### How is this safe?

Borrowing in Inko works as follows: each heap allocated value stores a borrow
counter. This counter is incremented when borrowing the value, and decremented
when the borrow is discarded. When dropping an owned value, the borrow counter
is checked and a panic is produced if the count is not zero.

Using this approach we can avoid having to implement a complex compile-time
borrow checking mechanism, and still implement patterns otherwise difficult or
impossible to implement when using borrow checking (i.e. linked lists or
graphs).

::: note
In the future, we intend to implement additional forms of compile-time analysis
to detect obvious cases where a value is dropped while still borrowed, reducing
the chances of encountering a runtime borrow error.
:::

::: note
While this approach incurs a small runtime cost, most of the borrow count
mutations can be optimized away. While we don't implement such optimizations at
the time of writing, we intend to do so in the future.
:::

This approach is not new and is in fact based on the paper ["Ownership You Can
Count On: A Hybrid Approach to Safe Explicit Memory
Management"](https://inko-lang.org/papers/ownership.pdf), originally published
in 2006 (the original source is no longer available).

While this approach may sound scary, in reality it's not as big of a deal as one
might think. Since the check is performed when dropping a value and not
when (for example) dereferencing a borrow, the behaviour is deterministic: if
the program triggers a borrow error with input A, then running the same program
with the same input 10 times will produce the same error 10 times. This,
combined with the stack trace that's displayed when encountering a borrow error,
makes it reasonably easy to debug and resolve such errors. We also found that
encountering borrow errors is rare to begin with as it just isn't that common
for borrows to outlive the owned values they borrow.

## Unique references

Inko also has a type of reference known as a "unique reference". Such references
impose heavy restriction on borrowing, which ensures that these borrows don't
exist when the unique reference is moved around. To illustrate this, change the
`cats.inko` program to the following:

```inko
import std.stdio (Stdout)

class Cat {}

class async Main {
  fn async main {
    let cats = recover [Cat(), Cat()]
    let borrow = ref cats

    Stdout.new.print('${cats.size} cats')
  }
}
```

Now run this using `inko run cats.inko`, and you should be presented with the
following compile-time error:

```
cats.inko:8:18 error(invalid-type): values of type 'uni ref Array[Cat]' can't be assigned to variables or fields
```

What happened is the following: using the `recover` keyword we turned the array
of cats into a _unique_ array of cats. When using `ref cats`, a special borrow
known as a "unique immutable borrow" (quite the mouthful) is created. The
compiler imposes restrictions on such borrows, such as not allowing them to be
assigned to variables.

Unique references are used when sending data between processes, as a value being
unique ensures no borrows to the data exist, and thus no race conditions can
occur when using the reference.

## Value types

OK I lied when I said Inko has four types of primary references, as I left out
one important one: value types. Value types are owned references that are copied
when they are moved, instead of transferring ownership. This allows you to use
both the old and new version. To illustrate, create `values.inko` with these
contents:

```inko
import std.stdio (Stdout)

class async Main {
  fn async main {
    let out = Stdout.new
    let a = 42
    let b = a

    out.print(a.to_string)
    out.print(b.to_string)
  }
}
```

Now run it using `inko run values.inko`, and the output is as follows:

```
42
42
```

The reason this program works is because `42` is an instance of the `Int` type,
which is a 64-bits signed integer, and `Int` is a value type.

Other value types are floats (`Float`), strings (`String`), processes, nil
(`Nil`), booleans (`Bool`), and C structures used as part of the FFI.
