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

When sending a message to an optional type `?T`, you can only send messages that
are supported by `T`. If the value happens to be `Nil`, another `Nil` is
produced. For example:

```inko
let x: ?Integer = Nil

x + 1 # => Nil
```

If `Nil` does implement a method for a message it receives, that method gets
called instead of returning `Nil`. For example:

```inko
let x: ?Integer = Nil

x.to_integer # => 0
```

A more realistic example is that of accessing a value from a nested array. Let's
say our array looks like this:

```inko
let numbers = Array.new(Array.new(Array.new(1, 2, 3)))
```

We can use `Array.get` to get a value from a potentially out of bounds index.
The return type of this method is `?Integer` for the above `numbers` Array. This
allows us to access nested values like so:

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
