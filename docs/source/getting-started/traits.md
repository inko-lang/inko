---
{
  "title": "Traits"
}
---

Inko types doesn't support type inheritance. Instead, code is shared between
types using "traits". Traits are essentially blueprints for types, specifying
what methods must be implemented and/or providing default implementations of
methods a type may wish to override.

Let's say we want to convert different type instances into strings. We can do
so using traits:

```inko
import std.stdio (Stdout)

trait ToString {
  fn to_string -> String
}

type Cat {
  let @name: String
}

impl ToString for Cat {
  fn to_string -> String {
    @name
  }
}

type async Main {
  fn async main {
    let garfield = Cat(name: 'Garfield')

    Stdout.new.print(garfield.to_string)
  }
}
```

Running this program produces the output "Garfield".

## Default methods

In this example, the `to_string` method in the `ToString` trait is a required
method. This means that any type that implements `ToString` _must_ provide an
implementation of the `to_string` method.

Default trait methods are defined as follows:

```inko
import std.stdio (Stdout)

trait ToString {
  fn to_string -> String {
    '...'
  }
}

type Cat {
  let @name: String
}

impl ToString for Cat {
  fn to_string -> String {
    @name
  }
}

type async Main {
  fn async main {
    let garfield = Cat(name: 'Garfield')

    Stdout.new.print(garfield.to_string)
  }
}
```

Here the default implementation of `to_string` is to return the string `'...'`.
Our implementation of `ToString` for `Cat` still overrides it, so the output is
still "Garfield". Because the method is a default method, we can use it as-is as
follows:

```inko
import std.stdio (Stdout)

trait ToString {
  fn to_string -> String {
    '...'
  }
}

type Cat {
  let @name: String
}

impl ToString for Cat {

}

type async Main {
  fn async main {
    let garfield = Cat(name: 'Garfield')

    Stdout.new.print(garfield.to_string)
  }
}
```

If we now run the program, the output is "...".

## Required traits

Traits can specify other traits that must be implemented before the trait itself
can be implemented:

```inko
trait ToString {
  fn to_string -> String
}

trait ToUpperString: ToString {
  fn to_upper_string -> String {
    to_string.to_upper
  }
}
```

Here the `ToUpperString` trait states that for a type to be able to implement
`ToUpperString`, it must also implement `ToString`.

You can also specify multiple required traits:

```inko
trait A {}
trait B {}
trait C: A + B {}
```

Here the trait `C` requires both `A` and `B` to be implemented.

## Conflicting trait methods

It's possible for different traits to define methods with the same name. If a
type tries to implement such traits, a compile-time error is produced. Inko
doesn't support renaming of trait methods as part of the implementation, so
you'll need to find a way to resolve such conflicts yourself.

## Conditional trait implementations

Sometimes we want to implement a trait, but only if additional requirements are
met. For example, we want to implement `std.cmp.Equal` for `Array` but only if
its sub values also implement `std.cmp.Equal`. This is done as follows:

```inko
import std.cmp (Equal)

impl Equal[ref Array[T]] for Array if T: Equal[ref T] {
  fn pub ==(other: ref Array[T]) -> Bool {
    ...
  }
}
```

What happens here is that we implement `Equal` over `ref Array[T]`, for any
`Array[T]` _provided_ that whatever is assigned to `T` also implements
`Equal[ref T]`. For example, given an `Array[User]`, the `Array.==` method is
only available if `User` implements `Equal[ref User]`.


## Traits, self, and Self

Within a default method, the type of `self` is
[`Self`](../references/core-types/#self). This type comes with the following
limitations:

- It can't be cast to other types, including parent traits
- It's not compatible with trait values (i.e. you can't pass it to an argument
  typed as a trait)
- Closures can't capture `self` if it's a borrow (`ref Self` or `mut Self`),
  only if it's an owned value (`Self`), and only by moving the value into the
  closure using an `fn move` closure

`Self` can also be used in the signature of trait methods, in which case it acts
as a placeholder for the type that implements the trait. For example,
`std.clone.Clone` uses it as follows:

```inko
trait pub Clone {
  fn pub clone -> Self
}
```

Thus if a type `User` implements `Clone`, calling `User.clone` returns a value
of type `User` and _not_ of type `Clone`.

If a trait uses `Self` in a method signature, types implementing the trait
_can't_ be cast to that trait:

```inko
trait Trait {
  fn example -> Self
}

type Type {}

impl Trait for Type {
  fn example -> Type {
    Type()
  }
}

Type() as Trait # => this is invalid
```
