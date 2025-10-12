---
{
  "title": "Tuples"
}
---

A tuple is a finite collection of values, possibly of different types. Tuples
are created using the syntax `(A, B, ...)` where `A`, `B` and `...` represent
the values stored in a tuple. For example:

```inko
(10, 20, 30)
```

This expression returns a tuple with three values: `10`, `20` and `30`. Unlike
arrays, the values don't need to be of the same type:

```inko
(10, 'hello', true)
```

Tuples are _finite_ meaning that they have a fixed length, unlike arrays which
can grow in size. This means there's no equivalent of `Array.push`, `Array.pop`
and so on.

## Accessing tuple values

Tuple values are accessed using their position, using the syntax `the_tuple.N`
where `N` is the index starting at `0`:

```inko
let pair = (10, 'hello')

pair.0 # => 10
pair.1 # => 'hello'
```

## Tuples are inline types

Tuples are `inline` types and thus are stored on the stack. This means it's not
possible to assign a new value to a tuple:

```inko
let pair = (10, 'hello')

pair.0 = 20 # => not valid, resulting in a compile-time error
```

## Limitations

The tuple syntax is syntax sugar for creating instances of the various tuple
types provided by the [](std.tuple) module. For example, the expression
`(10, 'hello')` is syntax sugar for creating an instance of
[](std.tuple.Tuple2). The `std.tuple` module only provides types for tuples with
up to 8 values, and thus tuples can only store at most 8 values.

::: tip
It's highly recommended to avoid creating tuples with more than three values.
Instead, use a custom type when you need to store more than three values.
:::
