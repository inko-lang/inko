# Types and methods

Inko provides two kinds of types: classes and traits. In this chapter we'll take
a look at how to define and use such types.

## Classes

Classes store state and provide methods, and are created using the `class`
keyword:

```inko
class Person {
  let @name: String
  let @age: Int
}
```

Instances of classes are created using the class literal syntax:

```inko
Person { @name = 'Alice', @age = 42 }
```

When creating an instance, all fields must be assigned a value, and a field
can't be assigned a value multiple times.

Classes don't support inheritance, and instead rely on traits to provide
reusable behaviour.

Classes come in three forms: regular classes, enum classes, and async classes.
Enum classes are algebraic data types created using the `class enum` syntax, and
its variants are specified using the `case` keyword:

```inko
class enum Error {
  case FileDoesntExit
  case PermissionDenied
}
```

When pattern matching against enum classes, the compiler ensures the match is
exhaustive.

Async classes are created using the `class async` syntax and are used to define
and spawn processes. This is covered in greater detail in the
[Concurrency](concurrency.md) chapter.

### Methods

Methods may be defined when defining the class or when reopening it:

```inko
class Person {
  let @name: String
  let @age: Int

  fn name -> String {
    @name.clone
  }
}

impl Person {
  fn age -> Int {
    @age.clone
  }
}
```

The default return type of a method is `Nil`. When the return type is `Nil`, any
expression implicitly returned in the method is ignored:

```inko
fn example {
  42
}

example # => nil
```

If a method is defined as returning `Nil`, you can't explicitly return a value
that isn't `Nil`:

```inko
fn example {
  return 42 # => Compile-time error
}
```

For enum classes the compiler generates a static method for each variant, using
the same name as the variant:

```inko
class enum Error {
  case FileDoesntExit
  case PermissionDenied
}

Error.FileDoesntExit # Same as Error.FileDoesntExit()
```

### Fields

Fields default to being private to the module the class is defined in. You can
make them public using `let pub`:

```inko
class Person {
  let pub @name: String
  let pub @age: Int
}
```

Fields are accessed using the same syntax as method calls, making it easier to
introduce custom getter/setter methods without having to change every line that
uses the fields:

```inko
let alice = Person { @name = 'Alice', @age = 42 }

alice.name # => 'Alice'
alice.age  # => 42
```

Enum classes can't define custom fields.

## Traits

Traits are a sort of contract for classes to adhere to: a trait can specify one
or more required methods as well as default methods. A class implementing a
trait must implement the required methods, and is automatically given a copy of
the default methods; unless the class overrides the implementation. Traits may
also list other traits a class must implement.

A simple example of a trait is `std::string::ToString`, defined as follows:

```inko
trait pub ToString {
  fn pub to_string -> String
}
```

A class implementing this trait _must_ provide a `to_string` implementation
compatible with the one of the trait. Here's what such an implementation might
look like:

```inko
import std::string::ToString

class Person {
  let @name: String
  let @age: Int
}

impl ToString for Person {
  fn pub to_string -> String {
    @name.clone
  }
}
```

A class can only implement a trait once, even if the trait is generic.

## Type and method visibility

Types and methods default to being private to the module they are defined in,
and can be made public using the `pub` keyword. For example, a public class is
defined as follows:

```inko
fn pub foo {
  # ...
}
```

A private type can't be used in the signature of a public method or field.
Private types _can_ define public methods, which in practise means they're the
same as private methods. This is allowed so private types can implement traits
that expose public methods, without requiring the type to also be public.

## Core types

Inko provides various core types, such as `String`, `Int`, and `Array`.

Some of these types are value types, which means that when they are moved a copy
is created and then moved. This allows you to continue using the original value
after it would be moved.

### Int

The `Int` class is used for integers. Integers are 64 bits signed integers.

`Int` is a value type.

### Float

The `Float` class is used for IEEE 754 double-precision floating point numbers.

`Float` is a value type.

### String

The `String` class is used for strings. Strings are UTF-8 encoded immutable
strings. Internally strings are represented such that they can be efficiently
passed to C code, at the cost of one extra byte of overhead per string.

`String` uses atomic reference counting when copying. This means that ten copies
of a 1 GiB `String` only require 1 GiB of memory.

`String` is a value type.

### Boolean

Inko's boolean type is `Boolean`. Instances of `Boolean` are created using
`true` and `false`.

`Boolean` is a value type.

### Array

`Array` is a contiguous growable array type and can store any value, as long as
all values in the array are of the same type.

### ByteArray

`ByteArray` is similar to `Array`, except its optimised for storing bytes. A
`ByteArray` needs less memory compared to an `Array`, but can only store `Int`
values in the range of 0 up to (and including) 255.

### Option

`Option` is an algebraic data type/enum class used to represent an optional
value. It has two variants: `Some(T)` and `None`, with `None` signalling the
lack of a value.

### Map

`Map` is a hash map and can store key-value pairs of any type, as long as the
keys implement the traits `std::hash::Hash` and `std::cmp::Equal`.

### Nil

`Nil` is Inko's unit type, and used to signal the complete lack of a value. The
difference with `Option` is that a value of type `Nil` can only ever be `Nil`,
not something else. `Nil` is used as the default return type of methods, and in
some cases can be used to explicitly ignore the result of an expression (e.g. in
pattern matching bodies).

`Nil` is a value type.

## Generic types

Types can be made generic, allowing them to operate on a wide range of types.
For example, here's how you'd might define a generic linked list:

```inko
class Node[T] {
  let @next: Option[Node[T]]
  let @value: T
}

class List[T] {
  let @head: Option[Node[T]]
  let @tail: Option[mut Node[T]]
}
```

Classes, traits, methods and variants can all be made generic. Here's how you'd
define a generic `Result` type commonly found in functional languages:

```inko
class enum Result[T, E] {
  case Ok(T)
  case Error(E)
}
```

## Type inference

Inko supports type inference, removing the need for type annotations in most
cases. For example, the type signature of an `Array` can be inferred based on
its usage:

```inko
# Here the compiler infers `a` as `Array[Int]`, because of the `push` below.
let a = []

a.push(42)
```

This works for any type, including generic types such as the `Option` type:

```inko
# `a` is inferred as `Option[Int]`.
let mut a = Option.None

a = Option.Some(42)
```

If a generic type can't be inferred, the compiler produces an error. In this
case explicit type signatures are necessary:

```inko
let mut a: Option[Int] = Option.None
```

## The prelude

Inko automatically imports certain symbols into your modules. These symbols are
part of what is called "the prelude".

The prelude includes the following types and methods:

| Symbol              | Source module
|:--------------------|:------------------------------------------------------
| `Int`               | `std::int`
| `Float`             | `std::float`
| `String`            | `std::string`
| `Array`             | `std::array`
| `Boolean`           | `std::bool`
| `Nil`               | `std::nil`
| `ByteArray`         | `std::byte_array`
| `Option`            | `std::option`
| `Map`               | `std::map`
| `panic`             | `std::process`
