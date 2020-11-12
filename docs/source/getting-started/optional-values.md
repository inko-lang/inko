# Optional values

In the section [Optional types](basic-types.md#optional-types) we briefly
covered optional types and values. We'll cover these in greater detail below.

When using an optional type `?T`, you can pass both an instance of a `T` and
`Nil` to it. For example:

```inko
let number: ?Integer = Nil
```

Here `number` is of type `?Integer`, so we can assign both `Nil` and a `Integer`
to it.

## Sending messages

When sending a message to an optional type `?T`, only messages supported by both
instances of `T` and `Nil` are available. `Nil` supports receiving any message,
and will return another `Nil` if there is no method for the message sent. For
example:

```inko
Nil.foo # => Nil
```

If `Nil` does implement a method for a message it receives, that method gets
called instead of returning `Nil`. For example:

```inko
Nil.to_integer # => 0
Nil.to_string  # => ''
```

A more realistic example is that of accessing a value from a nested array. Let's
say our array looks like this:

```inko
let numbers = Array.new(Array.new(Array.new(1, 2, 3)))
```

When sending `get` to an `Array`, the return value is `Nil` if the index is out
of bounds. `Nil` in turn doesn't implement the `get` method, meaning that
sending `get` to `Nil` produces another `Nil`. Thus, we can access a value from
the above array like so:

```inko
numbers.get(0).get(0).get(1) # => 2
```

In other languages, you'd have to write something like this:

```ruby
numbers = [[[1, 2, 3]]]

if numbers[0]
  if numbers[0][0]
    numbers[0][0][1]
  end
end
```

Inko's approach means that we don't have to explicitly check for a lack of a
value every time an optional value is produced. Instead, we can do so at the
end.

## Converting optional types

An optional type `?T` can be converted to a `T` in one of two ways:

1. Using the postfix `!` operator
1. Using an explicit type cast

Using the above array example, we can convert our `?Integer` to a `Integer`
using the postfix operator like so:

```inko
numbers.get(0).get(0).get(1)!
```

Using an explicit type cast, it looks like this:

```inko
numbers.get(0).get(0).get(1) as Integer
```

Inko doesn't perform runtime checks when casting types like this, so you should
use this with caution.
