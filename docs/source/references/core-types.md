---
{
  "title": "Core types"
}
---

Inko provides various core types, such as `String`, `Int`, and `Array`.

Some of these types are value types, which means that when they are moved a copy
is created and then moved.

## Array

`Array` is a contiguous growable array type and can store any value, as long as
all values in the array are of the same type.

## Bool

Inko's boolean type is `Bool`. Instances of `Bool` are created using `true` and
`false`.

`Bool` is a value type.

## ByteArray

`ByteArray` is similar to `Array`, except it's optimised for storing bytes. A
`ByteArray` needs less memory compared to an `Array`, but can only store `Int`
values in the range of 0 up to (and including) 255.

## Float

The `Float` class is used for IEEE 754 double-precision floating point numbers.

`Float` is a value type.

## Int

The `Int` class is used for integers. Integers are 64 bits signed integers.

`Int` is a value type.

## Map

`Map` is a hash map and can store key-value pairs of any type, as long as the
keys implement the traits `std.hash.Hash` and `std.cmp.Equal`.

## Nil

`Nil` is Inko's unit type, and used to signal the complete lack of a value. The
difference with `Option` is that a value of type `Nil` can only ever be `Nil`,
not something else. `Nil` is used as the default return type of methods, and in
some cases can be used to explicitly ignore the result of an expression (e.g. in
pattern matching bodies).

`Nil` is a value type.

## Option

`Option` is an algebraic data type/enum class used to represent an optional
value. It has two constructor: `Some(T)` and `None`, with `None` signalling the
lack of a value.

## Result

`Result` is an algebraic data type/enum class used for error handling. It has
two constructors: `Ok(T)` and `Error(E)`. The `Ok` constructor signals the
success of an operation, while `Error` signals an error occurred.

## String

The `String` class is used for strings. Strings are UTF-8 encoded immutable
strings. Internally strings are represented such that they can be efficiently
passed to C code, at the cost of one extra byte of overhead per string.

`String` uses atomic reference counting when copying. This means that ten copies
of a 1 GiB `String` only require 1 GiB of memory.

`String` is a value type.

## Never

`Never` is a type that indicates something never happens. When used as a return
type, it means the method never returns. An example of this is
`std.process.panic()`: this method panics and thus returns a `Never`.

You'll likely never need to use this type directly.

::: info
The `Never` type can only be used as the return type of a method, and can't be
used as a generic type argument (e.g. `Option[Never]` is invalid).
:::
