# Basic types

Inko has a variety of basic types, which we will cover in this chapter. In
addition to these types you can define your own types, which we will cover
separately.

## Integers

The `Integer` type is used for numbers without fractions, such as `10` and
`9000`. The `Integer` type is an arbitrary precision integer type, meaning it
can store values of any type; both positive and negative. There are no types for
different integer sizes, such as 16 and 32 bits integers. Here are a few
examples of `Integer` values:

```inko
10
-100
1023748123478213478201934712084372108934721034
-12034702183471823472180413740891278340274098
```

To make integer literals (the numbers above) easier to read, you can separate
the digits using an underscore:

```inko
10
-100
1_023_748_123_478_213_478_201_934_712_084_372_108_934_721_034
1_2_3
```

The use of these underscores has no impact on the value, it's purely cosmetic.

You can also use hexadecimal notation for integers:

```inko
0xFFF
0xccc
0XFFFEEE
0xFFF_FFF
```

## Floats

The `Float` type is used for numbers with fractions. The `Float` type follows
the [IEEE 754](https://en.wikipedia.org/wiki/IEEE_754) floating-point standard,
and is a double-precision floating point type. Here are a few examples:

```inko
10.5
-1038743.3
103478213_348374.5
10e+5
10e-5
```

## Strings

For text there is the `String` type, which is an immutable UTF-8 encoded string
type. You can create a `String` using single quotes (`'`) or double quotes
(`"`):

```inko
'hello'
"hello"
```

When using single quotes, escape sequences such as `\n` and `\t` are literal
characters. In other words, `'\t'` is a `String` containing two bytes: 92 (`\`)
and 116 (`t`).

When using double quotes, Inko translates these sequences into other characters.
For example, the `String` `"\t"` is a `String` containing only a single byte: 9,
the tab character byte. When using double quotes, you can use the following
escape sequences:

| Sequence | Character represented
|:---------|:------------------------------------
| `\t`     | Horizontal tab
| `\n`     | Newline
| `\r`     | Carriage return
| `\e`     | Escape character (e.g. for ANSI escape sequences)
| `\0`     | NULL byte

You can also escape quotes themselves to treat them as literal quotes. For
example, to create a single-quoted string containing a single quote you'd use
the following:

```inko
'hello\'world'
```

This also works for double quotes:

```inko
"hello\"world"
```

## Booleans

The boolean types are `True` and `False`, which are instances of the `Boolean`
type. Most languages use dedicated keywords for booleans, such as `true` and
`false` in Ruby. In Inko, booleans are just constants.

## Arrays

The `Array` type stores values in a contiguous order. Unlike other languages,
there is no special syntax for creating an `Array`. Instead, you just send the
`new` message to the `Array` type and pass it the values you wish to store in
the `Array`:

```inko
Array.new             # An empty array
Array.new(10, 20, 30) # An array with 3 values
```

You can store any value in an `Array`, provided it's compatible with the other
values in the `Array`. In other words, the `Array` type is a generic type; but
more on that later. For now, know that code such as this isn't valid:

```inko
Array.new(10, 'foo', 20.5) # Value types are not compatible, so this will not work
```

## Byte arrays

While you can store bytes as a sequence of `Integer` values in an `Array`, this
is not efficient as every `Integer` requires 8 bytes of space. For this Inko
provides the `ByteArray` type, which stores bytes more efficiently. You create a
`ByteArray` just like you create an `Array`:

```inko
ByteArray.new             # An empty ByteArray
ByteArray.new(10, 20, 30) # A ByteArray containing 3 bytes
```

Since a `ByteArray` stores bytes, the smallest `Integer` value it allows is `0`,
while the largest allowed value is `255`. This means the following is not valid:

```inko
ByteArray.new(9000)
```

## Hash maps

The `Map` type is Inko's built-in hash map type. Similar to the `Array` type
there is no special syntax for hash maps, you just create them by sending
messages:

```inko
let map = Map.new

map['key'] = 'value'
```

This will create a `Map`, then set the `'key'` key to the value `'value'`. You
can also send the `set` message to `Map`, which will return the `Map` itself.
This is useful when you want to create a `Map` and set keys right away:

```inko
Map.new.set('key1', 'value1').set('key2', 'value2')
```

Just like the `Array` type, a `Map` is generic. For both keys and values you can
use any type, provided the keys and values are compatible with those already
set. This means the following is invalid:

```inko
let map = Map.new

map['foo'] = 'bar'
map[10] = 'baz'
```

Here `10` is an `Integer`, which is not compatible with the type of the `'foo'`
key (a `String`).

## Nil

The type `Nil` is used to represent the lack of a value. This type is an
instance of `NilType`, though direct use of `NilType` is rare. To use `Nil`,
just refer to it as-is:

```inko
let nothing = Nil
```

The `Nil` type is not the same as the `NULL` type seen in other languages. Like
any other object you can send messages to it. How this works and what that
results in is covered in a separate chapter.

## Block types

A block is a method, closure, or lambda.

To create a method, use the `def` keyword:

```inko
def hello {
  # ...
}
```

Method names are alphanumerical, can contain underscores, and may end in a
single question mark (`?`). Method names can also be single binary operators
such as `+` and `/`, and the special indexing operators `[]` and `[]=`. Here are
just a few examples:

```inko
def [](index: Integer) {}
def []=(index: Integer, value: Integer) {}
def /(value: Integer) {}
def foo {}
def foo_bar {}
def valid? {}
```

To create a closure, use the `do` keyword:

```inko
do {
  # ...
}
```

Closures can capture outer variables:

```inko
let number = 10

do {
  number # => 10
}
```

You can also leave out the `do` and use curly braces, provided the closure has
no arguments, no explicit return type, and no explicit throw type:

```inko
{
  # This is also a closure
}
```

To create a lambda, use the `lambda` keyword:

```inko
lambda {
  # ...
}
```

Lambdas can't capture variables. This means lambdas are basically anonymous
methods that you can pass around.

## Objects

Objects, also known as classes or structures in other languages, are Inko's
record types. To create an object, use the `object` keyword:

```inko
object Person {
  # ...
}
```

You can create an instance of such objects by sending the `new` message to the
object. For example:

```inko
object Person {
  # ...
}

Person.new # Creates a new Person instance
```

Unlike classes found in other languages, Inko objects do not support
inheritance.

## Traits

Traits are types used in Inko to compose behaviour. Traits are like interfaces
found in other languages, with the added feature of being able to define default
method implementations. To create a trait, use the `trait` keyword:

```inko
trait ToString {
  # ...
}
```

Inko uses nominal typing, meaning a trait is only implemented for an object when
the developer implements it explicitly. How to do this will be covered
separately.

## Optional types

An optional type `T` is a type that can be either `T` or `Nil`. The syntax for
these types is `?T`, with `T` being the type name. For example, an optional
`String` would be `?String`.

Optional types are only valid in a few places, such as type signatures. How to
effectively use these will be discussed separately.

## Self types

The type `Self` is a special type that can be used in a few places, such as type
signatures. The use of this type is best explained using a simple example:

```inko
object Number {
  def add(other: Self) -> Self {
    # ...
  }
}
```

Here the argument of `other` is defined to be of type `Self`, and so is the
return type of the `add` method. Since this method is defined on the type
`Number`, using `Self` is like saying "This is an instance of `Number`".

Using `Self` is useful in traits where you may not know the type of the object
that implements the trait, and you just want something that is of that same
type. For example, most operators specify that their argument is of type `Self`,
ensuring that `10 + 5` is valid but `10 + "foo"` is not.

## Never types

The type `Never` is used to signal that something will never happen, such as
when a method will never return (because it terminates the program for example).
This type can only be used in type signatures, such as when defining a method's
return type.
