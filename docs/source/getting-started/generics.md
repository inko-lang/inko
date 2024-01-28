---
{
  "title": "Generics"
}
---

Types and methods can be generic, meaning that instead of supporting a single
fixed type (e.g. `String`), they can operate on many different types. Take the
following class for example:

```inko
class Box {
  let @value: String
}
```

This `Box` type can only store `String` values, but what if we want to also
store `Float` or a custom type of sorts? Generic types and methods allow us to
solve this problem, without having to copy-paste large amounts of code.

## Generic types

Generic classes are defined as follows:

```inko
class Box[T] {
  let @value: T
}
```

By defining the type as `Box[T]` instead of just `Box`, we've made it generic.
In this particular case the type defines a single generic type parameter: `T`,
of which the name is arbitrary (e.g. we could've picked `Kittens`, `VALUE`, or
something else). The type of the `value` field is defined as `T`, instead of
`String` or `Float`. We can now create instances of this type that use different
types for the `value` field:

```inko
Box { @value = 'test' }
Box { @value = 42 }
Box { @value = 1.123 }
```

We can of course define more than just one type parameter:

```inko
class Pair[A, B] {
  let @a: A
  let @b: B
}

Pair { @a = 'test', @b = 42 }
```

Traits are made generic in the same way:

```inko
trait ToBox[T] {
  fn to_box -> Box[T]
}
```

## Generic methods

Just like types, methods can be made generic. Take this method for example:

```inko
fn box(value: String) {
  ...
}
```

Like the `Box` type we started with, `value` is typed as `String` and thus only
`String` values can be passed as arguments to this method. Turning this method
into a generic method is done similar to making types generic:

```inko
fn box[T](value: T) {
  ...
}
```

Now the `box` method accepts values of different types such as `String`,
`Float`, and more.

We can also define multiple type parameters just as we can with generic types:

```inko
fn example[A, B](value: A, value: B) {
  ...
}
```

Return types can also be generic:

```inko
fn example[T](value: T) -> T {
  value
}
```

This method takes a value of any type and returns it as-is.

## Type parameter requirements

In these examples the type parameters don't specify any sort of requirements a
type must meet before it's considered compatible with the type parameter. As
such, there's not much we can do with values of these types (other than move
them around), as they could be anything. To resolve this, we can define one or
more required traits when defining a type parameter:

```inko
trait ToString {
  fn to_string -> String
}

class Box[T: ToString] {
  let @value: T
}
```

Here `T: ToString` means that for a type to be compatible with `T`, it must
implement the `ToString` trait. If you try to assign a value of which the type
doesn't implement `ToString`, you'll get a compile-time error.

Type parameters can define multiple required traits as follows:

```inko
trait A {}
trait B {}

class Box[T: A + B] {
  let @value: T
}
```

Here `T: A + B` means `T` requires both the traits `A` and `B` to be implemented
before a type is considered compatible with the type parameter.

The required traits can also be made generic:

```inko
trait Equal[A] {}

class Example[B: Equal[B]] {
  ...
}
```

In this example `B: Equal[B]` means that for a type `Foo` to be compatible with
`B`, it must implement the trait `Equal[Foo]`.
